use super::{
    dispatch_mark_error::DispatchMarkError,
    manifest_prune_service::ManifestPruneService,
    pending_manifest::PendingManifest,
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
    stored_manifest::StoredManifest,
    thread_identity::valid_id,
    thread_index_service::recent_thread_updates,
};
use crate::storage;
use std::{
    collections::HashSet,
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
            if valid_unique_targets(&targets) {
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
    if targets.is_empty() {
        return Ok(());
    }
    let updates = recent_thread_updates(home, targets)?;
    ManifestPruneService::run_with(home, targets, |id| Ok(updates.get(id).copied()))
}

#[cfg(test)]
pub(super) fn prune_ineligible_targets_with(
    home: &Path,
    targets: &mut Vec<PendingTarget>,
    updated_at: impl FnMut(&str) -> Result<Option<i64>, String>,
) -> Result<(), String> {
    ManifestPruneService::run_with(home, targets, updated_at)
}

pub(super) fn finalize_target(
    targets: &mut Vec<PendingTarget>,
    id: &str,
    owner_unavailable: bool,
    dispatched: bool,
    account_id: Option<&str>,
    preserve_binding: bool,
) {
    if preserve_binding && !dispatched {
        // The original account binding stays intact until that account is
        // active again or the ordinary eligibility window expires. This also
        // covers an explicit request that claimed a deferred target in memory.
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
pub(super) fn mark_dispatch_attempt(id: &str, mode: RecoveryMode) -> Result<(), DispatchMarkError> {
    let deferred = load_manifest()
        .map_err(DispatchMarkError::Other)?
        .iter()
        .any(|target| target.id == id && target.awaiting_owner);
    let binding = recovery_account_binding(deferred);
    mark_dispatch_attempt_for_account(id, binding.as_deref(), mode)
}

pub(super) fn recovery_account_binding(deferred: bool) -> Option<String> {
    let cli = current_account_binding();
    let desktop = if deferred {
        super::desktop_account_binding_service::DesktopAccountBindingService::verified(
            cli.as_deref(),
        )
    } else {
        None
    };
    super::desktop_account_binding_service::choose_recovery_account_binding(
        cli.as_deref(),
        desktop.as_deref(),
        deferred,
    )
}

pub(super) fn mark_dispatch_attempt_for_account(
    id: &str,
    binding: Option<&str>,
    mode: RecoveryMode,
) -> Result<(), DispatchMarkError> {
    let mut targets = load_manifest().map_err(DispatchMarkError::Other)?;
    let Some(target) = targets.iter().find(|target| target.id == id) else {
        return Err(DispatchMarkError::Other(
            "Recovery checkpoint disappeared before IPC dispatch".into(),
        ));
    };
    // An explicit request claimed this target; the binding guards only
    // unattended retries (see `recovery_checkpoint`).
    if target.awaiting_owner
        && mode != RecoveryMode::ExplicitTarget
        && (binding.is_none() || target.owner_account_id.as_deref() != binding)
    {
        return Err(DispatchMarkError::AccountChanged);
    }
    targets.retain(|target| target.id != id);
    write_manifest(&targets).map_err(DispatchMarkError::Other)
}

/// Resolve the uniquely matched active auth identity without exposing tokens.
pub(super) fn current_account_binding() -> Option<String> {
    super::active_auth_binding_service::ActiveAuthBindingService::current()
}

pub fn load_pending() -> Result<Vec<String>, String> {
    let home = storage::codex_home();
    let mut targets = load_manifest()?;
    // This caller only returns restart targets. Deferred ownerless targets are
    // handled by the probe worker and must not trigger a full checkpoint scan
    // or SQLite retries on each ordinary thread-detection pass.
    targets.retain(|target| !target.awaiting_owner);
    prune_ineligible_targets(&home, &mut targets)?;
    // Read-only: writing this snapshot could resurrect a consumed target.
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
    if !valid_unique_targets(targets) {
        return Err("Invalid or duplicate recovery thread ID".into());
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

fn valid_unique_targets(targets: &[PendingTarget]) -> bool {
    let mut ids = HashSet::with_capacity(targets.len());
    targets
        .iter()
        .all(|target| valid_id(&target.id) && ids.insert(target.id.as_str()))
}
