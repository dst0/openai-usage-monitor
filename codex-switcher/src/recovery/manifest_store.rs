use super::{
    dispatch_mark_error::DispatchMarkError, pending_manifest::PendingManifest,
    pending_target::PendingTarget, queue_snapshot::pending_count, stored_manifest::StoredManifest,
    thread_identity::valid_id,
};
use crate::{storage, switcher};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

pub(super) fn load_manifest() -> Result<Vec<PendingTarget>, String> {
    let path = storage::codex_home().join("desktop-recovery.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            let stored: StoredManifest =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let targets = match stored {
                StoredManifest::Current(manifest) if manifest.version == 1 => manifest.targets,
                StoredManifest::Current(_) => {
                    return Err("Unsupported recovery manifest version".into())
                }
                StoredManifest::Legacy(ids) => ids
                    .into_iter()
                    .map(|id| PendingTarget {
                        id,
                        offset: None,
                        awaiting_owner: false,
                        captured_restart: false,
                        owner_account_id: None,
                    })
                    .collect(),
            };
            if targets.iter().all(|target| valid_id(&target.id)) {
                Ok(targets)
            } else {
                Err("Invalid recovery manifest".into())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

pub(super) fn prune_ineligible_targets(
    home: &Path,
    targets: &mut Vec<PendingTarget>,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let mut eligible = Vec::new();
    for target in targets.iter() {
        if !valid_id(&target.id) || !switcher::is_user_thread(home, &target.id) {
            continue;
        }
        let is_recent = switcher::get_thread_updated_at(home, &target.id)
            .map(|updated| (now - updated).abs() <= switcher::RECENT_QUOTA_WINDOW_SECS)
            .unwrap_or(false);
        if !is_recent {
            continue;
        }
        let retain = match switcher::inspect_thread_rollout_state(home, &target.id) {
            switcher::ThreadRolloutState::ActiveInProgress
            | switcher::ThreadRolloutState::InterruptedByQuota
            | switcher::ThreadRolloutState::TurnAborted => true,
            switcher::ThreadRolloutState::CleanCompleted => pending_count(home, &target.id)? > 0,
            switcher::ThreadRolloutState::Unknown => false,
        };
        if retain {
            eligible.push(target.clone());
        }
    }
    *targets = eligible;
    Ok(())
}

pub(super) fn finalize_target(
    targets: &mut Vec<PendingTarget>,
    id: &str,
    owner_unavailable: bool,
    dispatched: bool,
    account_id: Option<&str>,
    account_mismatch: bool,
) {
    if account_mismatch && !dispatched {
        // The original account binding stays intact until that account is
        // active again or the ordinary eligibility window expires.
    } else if owner_unavailable && !dispatched && account_id.is_some() {
        if let Some(target) = targets.iter_mut().find(|target| target.id == id) {
            target.awaiting_owner = true;
            target.owner_account_id = account_id.map(str::to_owned);
        }
    } else {
        // Once a request might have reached Desktop, retrying risks a duplicate.
        targets.retain(|target| target.id != id);
    }
}

pub(super) fn validate_target_account_binding(
    targets: &[PendingTarget],
    ids: &[String],
    current_account: Option<&str>,
) -> Result<(), String> {
    for id in ids {
        if let Some(target) = targets
            .iter()
            .find(|target| target.id == *id && target.awaiting_owner)
        {
            if current_account.is_none() || target.owner_account_id.as_deref() != current_account {
                return Err("Deferred recovery belongs to a different Desktop account".into());
            }
        }
    }
    Ok(())
}

/// Remove the retry intent durably before an owner-routed request can be sent.
/// A crash after this point has an unknown outcome and must not redispatch.
pub(super) fn mark_dispatch_attempt(id: &str) -> Result<(), DispatchMarkError> {
    let binding = current_account_binding();
    mark_dispatch_attempt_for_account(id, binding.as_deref())
}

pub(super) fn mark_dispatch_attempt_for_account(
    id: &str,
    binding: Option<&str>,
) -> Result<(), DispatchMarkError> {
    let mut targets = load_manifest().map_err(DispatchMarkError::Other)?;
    let Some(target) = targets.iter().find(|target| target.id == id) else {
        return Err(DispatchMarkError::Other(
            "Recovery checkpoint disappeared before IPC dispatch".into(),
        ));
    };
    if target.awaiting_owner && (binding.is_none() || target.owner_account_id.as_deref() != binding)
    {
        return Err(DispatchMarkError::AccountChanged);
    }
    targets.retain(|target| target.id != id);
    write_manifest(&targets).map_err(DispatchMarkError::Other)
}

/// Confirm both the configured active account and the actual Desktop auth
/// identity without logging or persisting tokens or email addresses.
pub(super) fn current_account_binding() -> Option<String> {
    let accounts = storage::load_accounts().ok()?;
    let active_id = accounts.active_account_id?;
    let active = accounts
        .accounts
        .iter()
        .find(|account| account.id == active_id)?;
    let auth = storage::read_active_auth_json().ok()?;
    let tokens = auth.tokens.as_ref()?;
    let (email, _) = crate::oauth::extract_jwt_metadata_from_tokens(tokens);
    if tokens.account_id.as_deref() != Some(active.account_id.as_str())
        || !email?.eq_ignore_ascii_case(&active.email)
    {
        return None;
    }
    Some(active_id)
}

pub fn load_pending() -> Result<Vec<String>, String> {
    let home = storage::codex_home();
    let mut targets = load_manifest()?;
    prune_ineligible_targets(&home, &mut targets)?;
    // Read-only: callers may run during recovery, whose operation lock owns
    // manifest writes. Writing an old snapshot here could resurrect a target.
    Ok(targets
        .into_iter()
        .filter(|target| !target.awaiting_owner)
        .map(|target| target.id)
        .collect())
}

pub fn load_ownerless_pending() -> Result<Vec<String>, String> {
    Ok(load_manifest()?
        .into_iter()
        .filter(|target| target.awaiting_owner)
        .map(|target| target.id)
        .collect())
}

pub(super) fn write_manifest(targets: &[PendingTarget]) -> Result<(), String> {
    if !targets.iter().all(|target| valid_id(&target.id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let manifest_path = home.join("desktop-recovery.json");
    if targets.is_empty() {
        match std::fs::remove_file(&manifest_path) {
            Ok(()) => File::open(&home)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        return Ok(());
    }
    let tmp = home.join(format!("desktop-recovery.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let manifest = PendingManifest {
        version: 1,
        targets: targets.to_vec(),
    };
    let result = (|| {
        file.write_all(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &manifest_path).map_err(|e| e.to_string())?;
        File::open(&home)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Small atomic restart journal. Contains task/account identifiers and byte
/// offsets only, never prompts, transcript content, tokens, or credentials.
pub fn save_pending(ids: &[String]) -> Result<(), String> {
    if !ids.iter().all(|id| valid_id(id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let mut targets = load_manifest()?
        .into_iter()
        .filter(|target| target.awaiting_owner)
        .collect::<Vec<_>>();
    for id in ids {
        if targets.iter().any(|target| target.id == *id) {
            continue;
        }
        targets.push(PendingTarget {
            id: id.clone(),
            offset: switcher::find_thread_rollout_path(&home, id)
                .and_then(|path| path.metadata().ok().map(|metadata| metadata.len())),
            awaiting_owner: false,
            captured_restart: true,
            owner_account_id: None,
        });
    }
    write_manifest(&targets)
}
