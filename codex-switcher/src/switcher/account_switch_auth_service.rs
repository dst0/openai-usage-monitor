use super::active_auth_registry_sync_service::ActiveAuthRegistrySyncService;
use super::codex_availability_service::CodexAvailabilityService;
use super::direct_switch_journal::DirectSwitchJournal;
use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{codex_home, load_accounts, read_active_auth_json};
use chrono::Utc;

pub(super) struct AccountSwitchAuthService;

impl AccountSwitchAuthService {
    pub(super) fn replace(
        target: &AccountConfig,
        app_was_running: bool,
        auth_before_stop: Option<AuthJson>,
        accounts: &mut AccountsFile,
    ) -> Result<(Option<AuthJson>, AuthJson), String> {
        let prior_existed = auth_before_stop.is_some() || app_was_running;
        let original_active_id = accounts.active_account_id.clone();
        let previous = if app_was_running {
            ActiveAuthRegistrySyncService::sync_from_disk(accounts)
                .and_then(|auth| {
                    auth.ok_or("Desktop authentication disappeared after shutdown".into())
                })
                .map_err(CodexAvailabilityService::relaunch_previous_state)?
        } else {
            auth_before_stop.unwrap_or(AuthJson {
                auth_mode: Some("chatgpt".into()),
                openai_api_key: None,
                tokens: None,
                last_refresh: None,
                extra: Default::default(),
            })
        };
        if !app_was_running {
            *accounts = load_accounts()?;
            if accounts.active_account_id != original_active_id {
                return Err("Active account changed before direct switch auth write".into());
            }
        }
        let prior_active_id = accounts.active_account_id.clone();
        let fresh_target = Self::resolve_fresh_target(accounts, target).map_err(|error| {
            Self::abort_before_auth_write(
                app_was_running,
                &previous,
                prior_active_id.as_deref(),
                error,
            )
        })?;
        let committed = Self::prepare_replacement(&previous, &fresh_target);
        let home = codex_home();
        DirectSwitchJournal::begin(
            &home,
            prior_active_id.as_deref(),
            &fresh_target,
            prior_existed.then_some(&previous),
            &committed,
        )
        .map_err(|error| {
            Self::abort_before_auth_write(
                app_was_running,
                &previous,
                prior_active_id.as_deref(),
                error,
            )
        })?;

        if let Err(error) = CodexAvailabilityService::replace_auth_for_switch(
            prior_existed.then_some(&previous),
            &committed,
        ) {
            return Err(Self::finish_failed_auth_write(
                &home,
                app_was_running,
                error,
            ));
        }
        let readback = read_active_auth_json();
        if readback.as_ref() != Ok(&committed) {
            let error = "Switched authentication could not be verified after replacement".into();
            let restored = CodexAvailabilityService::restore_auth_without_relaunch(
                prior_existed.then_some(&previous),
                &committed,
                error,
            );
            return Err(Self::finish_failed_auth_write(
                &home,
                app_was_running,
                restored,
            ));
        }
        Ok((prior_existed.then_some(previous), committed))
    }

    fn resolve_fresh_target(
        accounts: &AccountsFile,
        selected: &AccountConfig,
    ) -> Result<AccountConfig, String> {
        let mut matches = accounts
            .accounts
            .iter()
            .filter(|account| account.id.eq_ignore_ascii_case(&selected.id));
        let fresh = matches
            .next()
            .ok_or("Selected account disappeared before auth write")?;
        if matches.next().is_some() {
            return Err("Selected account became ambiguous before auth write".into());
        }
        if fresh.tokens != selected.tokens
            || fresh.email != selected.email
            || fresh.account_id != selected.account_id
            || fresh.enabled != selected.enabled
            || fresh.needs_relogin()
        {
            return Err("Selected account credentials changed before auth write".into());
        }
        Ok(fresh.clone())
    }

    fn finish_failed_auth_write(
        home: &std::path::Path,
        app_was_running: bool,
        error: String,
    ) -> String {
        if let Err(uncertain) = DirectSwitchJournal::verify_prior_and_clear(home) {
            return format!(
                "{error}; direct switch intent is unresolved: {uncertain}; Desktop remains stopped"
            );
        }
        if app_was_running {
            CodexAvailabilityService::relaunch_previous_state(error)
        } else {
            error
        }
    }

    fn abort_before_auth_write(
        app_was_running: bool,
        previous: &AuthJson,
        expected_active_id: Option<&str>,
        error: String,
    ) -> String {
        Self::abort_before_auth_write_with(
            app_was_running,
            previous,
            expected_active_id,
            error,
            CodexAvailabilityService::relaunch_previous_state,
        )
    }

    fn abort_before_auth_write_with(
        app_was_running: bool,
        previous: &AuthJson,
        expected_active_id: Option<&str>,
        error: String,
        relaunch: impl FnOnce(String) -> String,
    ) -> String {
        if !app_was_running {
            return error;
        }
        let verified = (|| {
            let active = read_active_auth_json()?;
            let registry = load_accounts()?;
            let active_id = expected_active_id.ok_or("Previous account identity is unavailable")?;
            let mut matching = registry
                .accounts
                .iter()
                .filter(|account| account.id == active_id);
            let previous_account = matching.next().ok_or("Previous account disappeared")?;
            Ok::<bool, String>(
                active == *previous
                    && registry.active_account_id.as_deref() == Some(active_id)
                    && matching.next().is_none()
                    && active.tokens.as_ref() == Some(&previous_account.tokens),
            )
        })();
        match verified {
            Ok(true) => relaunch(error),
            _ => format!(
                "{error}; prior auth/registry could not be verified; Desktop remains stopped"
            ),
        }
    }

    pub(super) fn rollback_after_commit_failure(
        previous: Option<&AuthJson>,
        committed: &AuthJson,
        app_was_running: bool,
        error: String,
    ) -> String {
        Self::rollback_after_commit_failure_with(
            previous,
            committed,
            app_was_running,
            error,
            CodexAvailabilityService::restore_auth_without_relaunch,
            DirectSwitchJournal::verify_prior_and_clear,
            CodexAvailabilityService::relaunch_previous_state,
        )
    }

    fn rollback_after_commit_failure_with(
        previous: Option<&AuthJson>,
        committed: &AuthJson,
        app_was_running: bool,
        error: String,
        restore: impl FnOnce(Option<&AuthJson>, &AuthJson, String) -> String,
        verify_prior: impl FnOnce(&std::path::Path) -> Result<(), String>,
        relaunch: impl FnOnce(String) -> String,
    ) -> String {
        let restored = restore(previous, committed, error);
        if let Err(uncertain) = verify_prior(&codex_home()) {
            return format!(
                "{restored}; direct switch intent is unresolved: {uncertain}; Desktop remains stopped"
            );
        }
        if app_was_running {
            relaunch(restored)
        } else {
            restored
        }
    }

    fn prepare_replacement(previous: &AuthJson, target: &AccountConfig) -> AuthJson {
        let mut committed = previous.clone();
        committed.auth_mode = Some("chatgpt".into());
        committed.openai_api_key = None;
        committed.tokens = Some(target.tokens.clone());
        committed.last_refresh = Some(Utc::now().to_rfc3339());
        committed
    }
}

#[cfg(test)]
#[path = "account_switch_auth_service.test.rs"]
mod tests;
