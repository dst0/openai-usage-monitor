use super::{
    checkpoint_confirmation::CheckpointConfirmation,
    checkpoint_scan_cache::CachedCheckpointScan,
    manifest_store::{load_manifest, write_manifest},
    pending_target::PendingTarget,
    queue_snapshot::pending_count,
    thread_identity::valid_id,
};
#[cfg(test)]
use super::observer::Observer;
use crate::{storage, switcher};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const MAX_CACHED_CHECKPOINTS: usize = 128;
pub(super) const POST_CHECKPOINT_SCAN_BUDGET_BYTES: u64 = 16 * 1024 * 1024;
type CacheKey = (PathBuf, u64);
static SCANS: OnceLock<Mutex<HashMap<CacheKey, CachedCheckpointScan>>> = OnceLock::new();
static CONFIRMATIONS: OnceLock<Mutex<HashMap<CacheKey, CheckpointConfirmation>>> = OnceLock::new();

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

#[cfg(test)]
pub(super) fn confirmation_cursor_for(path: &Path, offset: u64) -> Option<u64> {
    CONFIRMATIONS
        .get()?
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(path.to_path_buf(), offset))
        .map(CheckpointConfirmation::cursor)
}

#[cfg(test)]
pub(super) fn post_checkpoint_status(home: &Path, target: &PendingTarget) -> Option<(bool, bool)> {
    let mut budget = POST_CHECKPOINT_SCAN_BUDGET_BYTES;
    post_checkpoint_status_with_budget(home, target, &mut budget)
}

pub(super) fn post_checkpoint_status_with_budget(
    home: &Path,
    target: &PendingTarget,
    budget: &mut u64,
) -> Option<(bool, bool)> {
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
    // Bound work while the global recovery operation lock is held. Incomplete
    // evidence cannot authorize an IPC dispatch; the next probe resumes here.
    let start = scan.observer.offset;
    let limit = metadata.len().min(start.saturating_add(*budget));
    *budget -= limit - start;
    let status = scan.scan_to(limit).ok();
    let after = path.metadata().ok();
    let identity_stable = after.as_ref().is_some_and(|after| {
        after.dev() == metadata.dev()
            && after.ino() == metadata.ino()
            && after.len() >= metadata.len()
            && (after.len() > metadata.len()
                || (after.modified().ok() == metadata.modified().ok()
                    && (after.ctime(), after.ctime_nsec())
                        == (metadata.ctime(), metadata.ctime_nsec())))
    });
    if !identity_stable || status.is_none() {
        scans.remove(&key);
        return None;
    }
    scan.update_identity(&metadata);
    if scan.observer.saw_oversized || scan.observer.saw_malformed {
        None
    } else if limit == metadata.len() && after.is_some_and(|after| after.len() == metadata.len()) {
        status
    } else {
        None
    }
}

/// Destructive journal changes require lifecycle proof from a fresh scan of
/// one unchanged rollout snapshot. Cached append evidence only starts this
/// confirmation; it never authorizes checkpoint replacement or pruning.
pub(super) fn confirmed_checkpoint_status_with_budget(
    home: &Path,
    target: &PendingTarget,
    budget: &mut u64,
) -> Option<(bool, bool)> {
    let offset = target.offset?;
    let path = switcher::find_thread_rollout_path(home, &target.id)?;
    let metadata = path.metadata().ok()?;
    if metadata.len() < offset {
        return None;
    }
    let key = (path.clone(), offset);
    let mut scans = CONFIRMATIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if scans.get(&key).is_some_and(|scan| !scan.matches(&metadata)) {
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
            CheckpointConfirmation::new(path.clone(), offset, &metadata)?,
        );
    }
    let scan = scans.get_mut(&key)?;
    let status = scan.scan_with_budget(budget);
    if !path
        .metadata()
        .ok()
        .is_some_and(|after| scan.matches(&after))
    {
        scans.remove(&key);
        return None;
    }
    status
}

pub(super) fn confirmed_snapshot_still_current(home: &Path, target: &PendingTarget) -> bool {
    let Some(offset) = target.offset else {
        return false;
    };
    let Some(path) = switcher::find_thread_rollout_path(home, &target.id) else {
        return false;
    };
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    CONFIRMATIONS
        .get()
        .and_then(|scans| scans.lock().ok())
        .and_then(|scans| {
            scans
                .get(&(path, offset))
                .map(|scan| scan.is_complete() && scan.matches(&metadata))
        })
        .unwrap_or(false)
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
