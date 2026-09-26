use super::{
    checkpoint_scan_cache::CachedCheckpointScan,
    manifest_store::{load_manifest, write_manifest},
    observer::Observer,
    pending_target::PendingTarget,
    thread_identity::valid_id,
};
use crate::{storage, switcher};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const MAX_CACHED_CHECKPOINTS: usize = 128;
const MAX_SCAN_BYTES_PER_PROBE: u64 = 16 * 1024 * 1024;
type CacheKey = (PathBuf, u64);
static SCANS: OnceLock<Mutex<HashMap<CacheKey, CachedCheckpointScan>>> = OnceLock::new();

#[cfg(test)]
pub(super) fn scanned_bytes_for(path: &Path, offset: u64) -> Option<u64> {
    SCANS
        .get()?
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(path.to_path_buf(), offset))
        .map(|scan| scan.scanned_bytes)
}

#[cfg(test)]
pub(super) fn cached_partial_capacity_for(path: &Path, offset: u64) -> Option<usize> {
    SCANS
        .get()?
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(path.to_path_buf(), offset))
        .map(|scan| scan.observer.partial.capacity())
}

pub(super) fn post_checkpoint_status(home: &Path, target: &PendingTarget) -> Option<(bool, bool)> {
    let offset = target.offset?;
    let path = switcher::find_thread_rollout_path(home, &target.id)?;
    let metadata = path.metadata().ok()?;
    if metadata.len() < offset {
        return None;
    }
    let key = (path.clone(), offset);
    let mut scans = SCANS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if scans
        .get(&key)
        .is_some_and(|scan| !scan.can_continue(&metadata))
    {
        scans.remove(&key);
    }
    if !scans.contains_key(&key) {
        if scans.len() >= MAX_CACHED_CHECKPOINTS {
            if let Some(evicted) = scans.keys().next().cloned() {
                scans.remove(&evicted);
            }
        }
        scans.insert(
            key.clone(),
            CachedCheckpointScan::new(path.clone(), offset, &metadata)?,
        );
    }
    let scan = scans.get_mut(&key)?;
    // Bound each probe, including its first pass over an older rollout. A
    // partial snapshot cannot prove a lifecycle outcome until the saved end
    // has been reached.
    let scan_end = metadata.len().min(
        scan.observer
            .offset
            .saturating_add(MAX_SCAN_BYTES_PER_PROBE),
    );
    let status = scan.scan_to(scan_end).ok();
    let after = path.metadata().ok();
    let identity_stable = after.as_ref().is_some_and(|after| {
        after.dev() == metadata.dev()
            && after.ino() == metadata.ino()
            && after.len() >= metadata.len()
            && (after.len() > metadata.len() || after.modified().ok() == metadata.modified().ok())
    });
    if !identity_stable || status.is_none() {
        scans.remove(&key);
        return None;
    }
    scan.update_identity(&metadata);
    if scan_end < metadata.len() {
        None
    } else {
        status
    }
}

/// Restart decisions must inspect the complete captured interval in one call.
/// A background probe may return `None` while its bounded scan is still in
/// progress; treating that as "no new turn" would retain a stale account
/// binding across another switch. This fresh read also confirms any cached
/// completion before an undispatched retry can be removed.
pub(super) fn post_checkpoint_status_fresh(
    home: &Path,
    target: &PendingTarget,
) -> Result<(bool, bool), String> {
    post_checkpoint_status_fresh_with(home, target, |_| {})
}

fn post_checkpoint_status_fresh_with(
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
    {
        return Err("Recovery rollout changed during checkpoint scan".into());
    }
    Ok((observer.evidence.started, observer.evidence.verified(None)))
}

#[cfg(test)]
pub(super) fn post_checkpoint_status_fresh_after_scan(
    home: &Path,
    target: &PendingTarget,
    after_scan: impl FnOnce(&Path),
) -> Result<(bool, bool), String> {
    post_checkpoint_status_fresh_with(home, target, after_scan)
}

/// Journal restart targets before shutdown, then call again after the old
/// Desktop exits. Preserve unrelated cold retries and their account binding.
/// A newly started turn supersedes an older retry for the same task.
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
        let older_retry = targets.iter().position(|target| target.id == *id);
        if let Some(index) = older_retry {
            let superseded = post_checkpoint_status_fresh(&home, &targets[index])?.0;
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
