use super::{
    pending_manifest::PendingManifest, pending_target::PendingTarget,
    stored_manifest::StoredManifest, thread_identity::valid_id,
};
use crate::{storage, switcher};
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

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
                    .map(|id| PendingTarget { id, offset: None })
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

pub(super) fn prune_ineligible_targets(home: &Path, targets: &mut Vec<PendingTarget>) {
    let now = chrono::Utc::now().timestamp();
    targets.retain(|target| {
        if !valid_id(&target.id) || !switcher::is_user_thread(home, &target.id) {
            return false;
        }
        let is_recent = switcher::get_thread_updated_at(home, &target.id)
            .map(|updated| (now - updated).abs() <= switcher::RECENT_QUOTA_WINDOW_SECS)
            .unwrap_or(false);
        if !is_recent {
            return false;
        }
        matches!(
            switcher::inspect_thread_rollout_state(home, &target.id),
            switcher::ThreadRolloutState::ActiveInProgress
                | switcher::ThreadRolloutState::InterruptedByQuota
                | switcher::ThreadRolloutState::TurnAborted
        )
    });
}

pub fn load_pending() -> Result<Vec<String>, String> {
    let home = storage::codex_home();
    let mut targets = load_manifest()?;
    let before_len = targets.len();
    prune_ineligible_targets(&home, &mut targets);
    if targets.len() != before_len {
        let _ = write_manifest(&targets);
    }
    Ok(targets.into_iter().map(|target| target.id).collect())
}

pub(super) fn write_manifest(targets: &[PendingTarget]) -> Result<(), String> {
    if !targets.iter().all(|target| valid_id(&target.id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let manifest_path = home.join("desktop-recovery.json");
    if targets.is_empty() {
        let _ = std::fs::remove_file(&manifest_path);
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
        std::fs::rename(&tmp, &manifest_path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Small atomic restart journal. Contains UUIDs and byte offsets only, never
/// prompts, transcript content, account data, or credentials.
pub fn save_pending(ids: &[String]) -> Result<(), String> {
    if !ids.iter().all(|id| valid_id(id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let targets = ids
        .iter()
        .map(|id| PendingTarget {
            id: id.clone(),
            offset: switcher::find_thread_rollout_path(&home, id)
                .and_then(|path| path.metadata().ok().map(|metadata| metadata.len())),
        })
        .collect::<Vec<_>>();
    write_manifest(&targets)
}
