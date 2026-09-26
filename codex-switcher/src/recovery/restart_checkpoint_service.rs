#[cfg(test)]
use super::observer::Observer;
use super::{
    checkpoint_scan_registry::CheckpointScanRegistry,
    manifest_store::{load_manifest, write_manifest},
    pending_target::PendingTarget,
    queue_snapshot::pending_count,
    thread_identity::valid_id,
};
use crate::{storage, switcher};
#[cfg(test)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) const POST_CHECKPOINT_SCAN_BUDGET_BYTES: u64 = 16 * 1024 * 1024;

#[cfg(test)]
pub(super) fn scanned_bytes_for(path: &Path, offset: u64) -> Option<u64> {
    CheckpointScanRegistry::shared().scanned_bytes_for(path, offset)
}

#[cfg(test)]
pub(super) fn cached_partial_capacity_for(path: &Path, offset: u64) -> Option<usize> {
    CheckpointScanRegistry::shared().cached_partial_capacity_for(path, offset)
}

#[cfg(test)]
pub(super) fn confirmation_cursor_for(path: &Path, offset: u64) -> Option<u64> {
    CheckpointScanRegistry::shared().confirmation_cursor_for(path, offset)
}

#[cfg(test)]
pub(super) fn post_checkpoint_status(home: &Path, target: &PendingTarget) -> Option<(bool, bool)> {
    let mut budget = POST_CHECKPOINT_SCAN_BUDGET_BYTES;
    post_checkpoint_status_with_budget(home, target, &mut budget)
}

/// See `CheckpointScanRegistry::post_checkpoint_status_with_budget`; this
/// uses the process-wide registry.
pub(super) fn post_checkpoint_status_with_budget(
    home: &Path,
    target: &PendingTarget,
    budget: &mut u64,
) -> Option<(bool, bool)> {
    CheckpointScanRegistry::shared().post_checkpoint_status_with_budget(home, target, budget)
}

/// See `CheckpointScanRegistry::confirmed_checkpoint_status_with_budget`;
/// this uses the process-wide registry.
pub(super) fn confirmed_checkpoint_status_with_budget(
    home: &Path,
    target: &PendingTarget,
    budget: &mut u64,
) -> Option<(bool, bool)> {
    CheckpointScanRegistry::shared().confirmed_checkpoint_status_with_budget(home, target, budget)
}

pub(super) fn confirmed_snapshot_still_current(home: &Path, target: &PendingTarget) -> bool {
    CheckpointScanRegistry::shared().confirmed_snapshot_still_current(home, target)
}

#[cfg(test)]
pub(super) fn post_checkpoint_status_fresh_after_scan(
    home: &Path,
    target: &PendingTarget,
    after_scan: impl FnOnce(&Path),
) -> Result<(bool, bool), String> {
    let offset = target
        .offset
        .ok_or("Recovery checkpoint offset is unavailable")?;
    let path = switcher::find_thread_rollout_path(home, &target.id)
        .ok_or("Recovery rollout is unavailable")?;
    let before = path.metadata().map_err(|error| error.to_string())?;
    let mut observer = Observer::checkpoint_at(path.clone(), offset)?;
    observer.poll_to(before.len())?;
    after_scan(&path);
    let after = path.metadata().map_err(|error| error.to_string())?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || after.len() != before.len()
        || after.modified().ok() != before.modified().ok()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || observer.saw_oversized
        || observer.saw_malformed
        || !observer.partial.is_empty()
    {
        return Err("Recovery rollout changed or was unreadable during checkpoint scan".into());
    }
    Ok((observer.evidence.started, observer.evidence.verified(None)))
}

/// Journal restart targets before shutdown, then call again after the old
/// Desktop exits. Preserve unrelated cold retries and their account binding.
/// Verified new work supersedes an older retry only when no follow-up remains.
pub fn save_pending(ids: &[String]) -> Result<(), String> {
    if !ids.iter().all(|id| valid_id(id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let mut targets = load_manifest()?
        .into_iter()
        .filter(|target| target.awaiting_owner)
        .collect::<Vec<_>>();
    let mut scan_budget = POST_CHECKPOINT_SCAN_BUDGET_BYTES;
    for id in ids {
        let older_retry = targets.iter().position(|target| target.id == *id);
        if let Some(index) = older_retry {
            let superseded = match post_checkpoint_status_with_budget(
                &home,
                &targets[index],
                &mut scan_budget,
            ) {
                Some((_, true)) => {
                    confirmed_checkpoint_status_with_budget(
                        &home,
                        &targets[index],
                        &mut scan_budget,
                    )
                    .is_some_and(|(_, verified)| verified)
                        && pending_count(&home, id)? == 0
                        && confirmed_snapshot_still_current(&home, &targets[index])
                }
                _ => false,
            };
            if !superseded {
                continue;
            }
            targets.remove(index);
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
