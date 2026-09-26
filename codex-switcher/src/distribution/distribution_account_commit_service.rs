use super::app_lifecycle::AppLifecycle;
use super::desktop_app_session::DesktopAppSession;
use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{self, read_active_auth_json};
use chrono::Utc;
use std::path::Path;

pub struct DistributionAccountCommitService;

impl DistributionAccountCommitService {
    pub fn validate_targets(
        accounts_file: &AccountsFile,
        app_target: Option<&str>,
        cli_target: Option<&str>,
    ) -> Result<(), String> {
        for target in [app_target, cli_target].into_iter().flatten() {
            Self::find_account(accounts_file, target)?;
        }
        Ok(())
    }

    pub fn find_account(
        accounts_file: &AccountsFile,
        query: &str,
    ) -> Result<AccountConfig, String> {
        let index = crate::switcher::resolve_target_account_idx(&accounts_file.accounts, query)?;
        Ok(accounts_file.accounts[index].clone())
    }

    pub fn apply_auth_tokens(
        lifecycle: &dyn AppLifecycle,
        account: &AccountConfig,
    ) -> Result<(AuthJson, AuthJson), String> {
        Self::apply_auth_tokens_with_hook(lifecycle, account, |_| Ok(()))
    }

    pub fn apply_auth_tokens_if_current(
        lifecycle: &dyn AppLifecycle,
        account: &AccountConfig,
        expected: &AuthJson,
    ) -> Result<(AuthJson, AuthJson), String> {
        Self::apply_auth_tokens_with_hook(lifecycle, account, |previous| {
            if previous != expected {
                return Err("Shared authentication changed after registry sync".into());
            }
            Ok(())
        })
    }

    fn apply_auth_tokens_with_hook(
        lifecycle: &dyn AppLifecycle,
        account: &AccountConfig,
        before_commit: impl FnOnce(&AuthJson) -> Result<(), String>,
    ) -> Result<(AuthJson, AuthJson), String> {
        let previous = read_active_auth_json()?;
        if previous.auth_mode.as_deref() != Some("chatgpt") {
            return Err("Active authentication mode is not ChatGPT".into());
        }
        let mut committed = previous.clone();
        committed.openai_api_key = None;
        committed.tokens = Some(account.tokens.clone());
        committed.last_refresh = Some(Utc::now().to_rfc3339());
        before_commit(&previous)?;
        // This is the last checked writer boundary before replacing shared
        // auth. Desktop may have appeared while candidates were evaluated.
        if lifecycle.is_app_running()? {
            return Err("Desktop writer appeared before shared authentication replacement".into());
        }
        storage::compare_and_write_active_auth_json_for_switch(&previous, &committed, || {
            lifecycle.is_app_running()
        })?;
        Self::verify_offline_auth(lifecycle, &committed)?;
        Ok((previous, committed))
    }

    /// A running Desktop can appear after the prewrite check because it does
    /// not honor the Monitor lock. A mismatch is an uncertain commit: callers
    /// retain the distribution journal and must not rewrite auth again.
    pub fn verify_offline_auth(
        lifecycle: &dyn AppLifecycle,
        expected: &AuthJson,
    ) -> Result<(), String> {
        if lifecycle.is_app_running()? {
            return Err("Desktop writer appeared after shared authentication replacement".into());
        }
        let observed = read_active_auth_json()?;
        if &observed != expected {
            return Err("Shared authentication changed after replacement".into());
        }
        if lifecycle.is_app_running()? {
            return Err("Desktop writer appeared during authentication readback".into());
        }
        Ok(())
    }

    pub fn restore_when_desktop_stopped(
        lifecycle: &dyn AppLifecycle,
        previous: &AuthJson,
        committed: &AuthJson,
    ) -> Result<(), String> {
        Self::restore_when_desktop_stopped_with_hook(lifecycle, previous, committed, || {})
    }

    fn restore_when_desktop_stopped_with_hook(
        lifecycle: &dyn AppLifecycle,
        previous: &AuthJson,
        committed: &AuthJson,
        before_commit: impl FnOnce(),
    ) -> Result<(), String> {
        if lifecycle.is_app_running()? {
            return Err("Desktop is running; authentication rollback would race its writer".into());
        }
        let current = read_active_auth_json()?;
        if &current != committed {
            return Err("Authentication changed after commit; rollback was not attempted".into());
        }
        before_commit();
        storage::compare_and_write_active_auth_json_for_switch(committed, previous, || {
            lifecycle.is_app_running()
        })?;
        Self::verify_offline_auth(lifecycle, previous)
    }

    pub fn rollback_and_relaunch_previous(
        lifecycle: &dyn AppLifecycle,
        home: &Path,
        accounts: &AccountsFile,
        expected_previous_id: &str,
        previous: &AuthJson,
        committed: &AuthJson,
    ) -> Result<(), String> {
        Self::restore_when_desktop_stopped(lifecycle, previous, committed)?;
        Self::relaunch_if_auth_identity_matches(lifecycle, home, accounts, expected_previous_id)
    }

    pub fn relaunch_if_auth_identity_matches(
        lifecycle: &dyn AppLifecycle,
        home: &Path,
        accounts: &AccountsFile,
        expected_previous_id: &str,
    ) -> Result<(), String> {
        if lifecycle.is_app_running()? {
            return Err("Desktop is already running".into());
        }
        let current = read_active_auth_json()?;
        let mut matched = accounts.clone();
        crate::switcher::ActiveAuthRegistrySyncService::reconcile(&mut matched, &current)?;
        if matched.active_account_id.as_deref() != Some(expected_previous_id) {
            return Err("Authentication identity changed before previous Desktop relaunch".into());
        }
        let observed = read_active_auth_json()?;
        if observed != current {
            return Err("Authentication changed during previous Desktop relaunch check".into());
        }
        let pids = lifecycle.launch_app()?;
        if pids.len() != 1 {
            return Err(format!(
                "Previous Desktop relaunch produced {} main processes",
                pids.len()
            ));
        }
        lifecycle.verify_desktop_stable(&pids, false)?;
        let pid = pids[0];
        let process = lifecycle.inspect_process(pid)?;
        let path = home.join("desktop-app-session.json");
        let previous = DesktopAppSession::load_checked(&path)?;
        let bound =
            DesktopAppSession::bound(expected_previous_id, expected_previous_id, process.clone());
        if let Err(error) = bound.save(&path) {
            let restore =
                DesktopAppSession::restore_after_failed_save(&path, previous.as_ref(), &bound);
            return Err(match restore {
                Ok(()) => error,
                Err(restore) => {
                    format!("{error}; previous Desktop marker rollback failed: {restore}")
                }
            });
        }
        let verification = (|| {
            if !lifecycle.is_app_running()?
                || lifecycle.inspect_process(pid)? != process
                || DesktopAppSession::load_checked(&path)?.as_ref() != Some(&bound)
            {
                return Err("Previous Desktop identity changed after relaunch".into());
            }
            let mut persisted = accounts.clone();
            Self::commit_latest_desktop_auth(
                lifecycle,
                home,
                &mut persisted,
                expected_previous_id,
            )?;
            if DesktopAppSession::load_checked(&path)?.as_ref() != Some(&bound) {
                return Err("Previous Desktop marker changed after account commit".into());
            }
            Ok(())
        })();
        if let Err(error) = verification {
            let restore =
                DesktopAppSession::restore_after_failed_save(&path, previous.as_ref(), &bound);
            return Err(match restore {
                Ok(()) => error,
                Err(restore) => {
                    format!("{error}; previous Desktop marker rollback failed: {restore}")
                }
            });
        }
        Ok(())
    }

    pub fn commit_latest_desktop_auth(
        lifecycle: &dyn AppLifecycle,
        home: &Path,
        accounts: &mut AccountsFile,
        expected_target_id: &str,
    ) -> Result<(), String> {
        Self::commit_latest_desktop_auth_with_hook(
            lifecycle,
            home,
            accounts,
            expected_target_id,
            || Ok(()),
            || Ok(()),
        )
    }

    fn commit_latest_desktop_auth_with_hook(
        lifecycle: &dyn AppLifecycle,
        home: &Path,
        accounts: &mut AccountsFile,
        expected_target_id: &str,
        before_registry_commit: impl FnOnce() -> Result<(), String>,
        after_registry_commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let session = DesktopAppSession::load_checked(&home.join("desktop-app-session.json"))?
            .ok_or("Relaunched Desktop has no valid account binding")?;
        let process = session
            .process
            .as_ref()
            .ok_or("Relaunched Desktop account binding has no process identity")?;
        if session.account_id != expected_target_id
            || session.cli_account_id.as_deref() != Some(expected_target_id)
            || !lifecycle.is_app_running()?
            || lifecycle.inspect_process(process.pid)? != *process
        {
            return Err("Relaunched Desktop account binding changed before registry commit".into());
        }
        before_registry_commit()?;
        let mut committed_auth = None;
        storage::update_accounts_atomically(|fresh| {
            let auth = storage::read_active_auth_json()?;
            crate::switcher::ActiveAuthRegistrySyncService::reconcile(fresh, &auth)?;
            if fresh.active_account_id.as_deref() != Some(expected_target_id) {
                return Err("Desktop authentication changed before registry commit".into());
            }
            committed_auth = Some(auth);
            Ok(())
        })?;
        let committed_auth =
            committed_auth.ok_or("Desktop authentication commit was not observed")?;
        after_registry_commit()?;
        // Desktop may rotate or replace its credentials during the registry
        // write. Only report a verified commit when the saved registry, live
        // auth, and exact relaunched process still agree after that write.
        let persisted = storage::load_accounts()?;
        let latest_auth = storage::read_active_auth_json()?;
        let mut identified = persisted.clone();
        crate::switcher::ActiveAuthRegistrySyncService::reconcile(&mut identified, &latest_auth)?;
        let saved_tokens = persisted
            .accounts
            .iter()
            .find(|account| account.id == expected_target_id)
            .map(|account| &account.tokens);
        if persisted.active_account_id.as_deref() != Some(expected_target_id)
            || identified.active_account_id.as_deref() != Some(expected_target_id)
            || saved_tokens != latest_auth.tokens.as_ref()
            || latest_auth != committed_auth
            || !lifecycle.is_app_running()?
            || lifecycle.inspect_process(process.pid)? != *process
        {
            return Err("Relaunched Desktop account commit changed during verification".into());
        }
        if storage::read_active_auth_json()? != latest_auth {
            return Err("Relaunched Desktop authentication changed during final readback".into());
        }
        *accounts = persisted;
        Ok(())
    }
}

#[cfg(test)]
#[path = "distribution_account_commit_service.test.rs"]
mod tests;
