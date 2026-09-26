use super::account_switch_commit_service::AccountSwitchCommitService;
use super::is_shared_auth_active_checked;
use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{auth_json_path, codex_home, load_accounts, read_active_auth_json};
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::Path;

#[path = "direct_switch_journal_store.rs"]
mod direct_switch_journal_store;
use direct_switch_journal_store::DirectSwitchJournalStore;

const VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectSwitchJournal {
    version: u8,
    nonce: String,
    previous_active_id: Option<String>,
    target_id: String,
    previous_auth_sha256: Option<String>,
    committed_auth_sha256: String,
    target_binding_sha256: String,
    started_at: String,
}

impl DirectSwitchJournal {
    pub(super) fn begin(
        home: &Path,
        previous_active_id: Option<&str>,
        target: &AccountConfig,
        previous_auth: Option<&AuthJson>,
        committed_auth: &AuthJson,
    ) -> Result<Self, String> {
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Direct switch intent nonce unavailable".to_string())?;
        let journal = Self {
            version: VERSION,
            nonce: format!("{:016x}", u64::from_ne_bytes(nonce)),
            previous_active_id: previous_active_id.map(str::to_owned),
            target_id: target.id.clone(),
            previous_auth_sha256: previous_auth.map(auth_digest).transpose()?,
            committed_auth_sha256: auth_digest(committed_auth)?,
            target_binding_sha256: target_binding_digest(target)?,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        journal.validate()?;
        DirectSwitchJournalStore::create(home, &journal)?;
        Ok(journal)
    }

    fn validate(&self) -> Result<(), String> {
        let valid_id =
            |id: &str| !id.is_empty() && id.len() <= 2048 && !id.chars().any(char::is_control);
        let valid_hash =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if self.version != VERSION
            || self.nonce.len() != 16
            || !self.nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !valid_id(&self.target_id)
            || self
                .previous_active_id
                .as_deref()
                .is_some_and(|id| !valid_id(id) || id == self.target_id)
            || self
                .previous_auth_sha256
                .as_deref()
                .is_some_and(|value| !valid_hash(value))
            || !valid_hash(&self.committed_auth_sha256)
            || !valid_hash(&self.target_binding_sha256)
            || chrono::DateTime::parse_from_rfc3339(&self.started_at).is_err()
        {
            return Err("Direct switch intent is invalid".into());
        }
        Ok(())
    }

    pub(super) fn verify_prior_and_clear(home: &Path) -> Result<(), String> {
        Self::reconcile_with(home, is_shared_auth_active_checked, false)
    }

    pub(super) fn verify_target_and_clear(home: &Path) -> Result<(), String> {
        Self::reconcile_with(home, is_shared_auth_active_checked, true)
    }

    pub(super) fn reconcile_with(
        home: &Path,
        mut writer_active: impl FnMut() -> Result<bool, String>,
        allow_target_commit: bool,
    ) -> Result<(), String> {
        let Some(journal) = DirectSwitchJournalStore::load(home)? else {
            return Ok(());
        };
        if writer_active()? {
            return Err("Direct switch intent awaits a stopped shared-auth writer".into());
        }
        let registry = load_accounts()?;
        let auth = read_optional_auth()?;
        if journal.matches_prior(&registry, auth.as_ref())? {
            if writer_active()? {
                return Err("Direct switch intent awaits a stopped shared-auth writer".into());
            }
            let fresh = load_accounts()?;
            let fresh_auth = read_optional_auth()?;
            if !journal.matches_prior(&fresh, fresh_auth.as_ref())? {
                return Err("Direct switch prior-state readback changed".into());
            }
            return DirectSwitchJournalStore::clear_if_matches(home, &journal);
        }
        if !allow_target_commit || !journal.matches_target(&registry, auth.as_ref())? {
            return Err("Direct switch intent requires account/auth reconciliation".into());
        }
        if registry.active_account_id.as_deref() == journal.previous_active_id.as_deref() {
            let target = unique_account(&registry, &journal.target_id)?;
            AccountSwitchCommitService::commit(target, journal.previous_active_id.as_deref())?;
        } else if registry.active_account_id.as_deref() != Some(journal.target_id.as_str()) {
            return Err("Active account changed during direct switch reconciliation".into());
        }
        let fresh = load_accounts()?;
        let fresh_auth = read_optional_auth()?;
        if fresh.active_account_id.as_deref() != Some(journal.target_id.as_str())
            || !journal.matches_target(&fresh, fresh_auth.as_ref())?
        {
            return Err("Direct switch reconciliation readback changed".into());
        }
        if writer_active()? {
            return Err("Direct switch intent awaits a stopped shared-auth writer".into());
        }
        DirectSwitchJournalStore::clear_if_matches(home, &journal)
    }

    fn matches_prior(
        &self,
        registry: &AccountsFile,
        auth: Option<&AuthJson>,
    ) -> Result<bool, String> {
        if registry.active_account_id != self.previous_active_id {
            return Ok(false);
        }
        match (auth, self.previous_auth_sha256.as_deref()) {
            (None, None) => Ok(true),
            (Some(auth), Some(expected)) if auth_digest(auth)? == expected => {
                let Some(previous_id) = self.previous_active_id.as_deref() else {
                    return Ok(false);
                };
                let previous = unique_account(registry, previous_id)?;
                Ok(auth.tokens.as_ref() == Some(&previous.tokens))
            }
            _ => Ok(false),
        }
    }

    fn matches_target(
        &self,
        registry: &AccountsFile,
        auth: Option<&AuthJson>,
    ) -> Result<bool, String> {
        let Some(auth) = auth else { return Ok(false) };
        let target = unique_account(registry, &self.target_id)?;
        Ok(target_binding_digest(target)? == self.target_binding_sha256
            && auth_digest(auth)? == self.committed_auth_sha256
            && auth.auth_mode.as_deref() == Some("chatgpt")
            && auth.openai_api_key.is_none()
            && auth.tokens.as_ref() == Some(&target.tokens))
    }
}

fn unique_account<'a>(registry: &'a AccountsFile, id: &str) -> Result<&'a AccountConfig, String> {
    let mut matches = registry.accounts.iter().filter(|account| account.id == id);
    let target = matches.next().ok_or("Direct switch account disappeared")?;
    if matches.next().is_some() {
        return Err("Direct switch account became ambiguous".into());
    }
    Ok(target)
}

fn auth_digest(auth: &AuthJson) -> Result<String, String> {
    let bytes = serde_json::to_vec(auth).map_err(|_| "Active auth digest failed".to_string())?;
    Ok(sha256_hex(&bytes))
}

fn target_binding_digest(target: &AccountConfig) -> Result<String, String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "id": target.id,
        "email": target.email,
        "account_id": target.account_id,
        "tokens": target.tokens,
        "enabled": target.enabled,
        "needs_relogin": target.needs_relogin(),
    }))
    .map_err(|_| "Target account digest failed".to_string())?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest(&SHA256, bytes).as_ref() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn read_optional_auth() -> Result<Option<AuthJson>, String> {
    match std::fs::symlink_metadata(auth_json_path()) {
        Ok(_) => read_active_auth_json().map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Direct switch auth path could not be inspected".into()),
    }
}

pub(crate) fn reconcile_pending_direct_switch() -> Result<(), String> {
    DirectSwitchJournal::reconcile_with(&codex_home(), is_shared_auth_active_checked, true)
}

#[cfg(test)]
pub(crate) fn create_direct_switch_intent_for_test(
    home: &Path,
    previous_active_id: Option<&str>,
    target: &AccountConfig,
    previous_auth: Option<&AuthJson>,
    committed_auth: &AuthJson,
) -> Result<(), String> {
    DirectSwitchJournal::begin(
        home,
        previous_active_id,
        target,
        previous_auth,
        committed_auth,
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "direct_switch_journal.test.rs"]
mod tests;
