use super::{
    pending_target::PendingTarget,
    queue_snapshot::pending_count,
    restart_checkpoint_service::{
        confirmed_checkpoint_status_with_budget, confirmed_snapshot_still_current,
        post_checkpoint_status_with_budget, POST_CHECKPOINT_SCAN_BUDGET_BYTES,
    },
    thread_identity::valid_id,
};
use crate::switcher::{self, ThreadRolloutState};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const MAX_ROTATION_HOMES: usize = 128;
static NEXT_OWNERLESS_SCAN: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();

pub(super) struct ManifestPruneService;

impl ManifestPruneService {
    pub(super) fn run_with(
        home: &Path,
        targets: &mut Vec<PendingTarget>,
        updated_at: impl FnMut(&str) -> Result<Option<i64>, String>,
    ) -> Result<(), String> {
        Self::run_with_inspector(
            home,
            targets,
            updated_at,
            switcher::inspect_thread_rollout_state,
        )
    }

    pub(super) fn run_with_inspector(
        home: &Path,
        targets: &mut Vec<PendingTarget>,
        mut updated_at: impl FnMut(&str) -> Result<Option<i64>, String>,
        mut inspect: impl FnMut(&Path, &str) -> ThreadRolloutState,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();
        let mut eligible = Vec::new();
        let mut scan_budget = POST_CHECKPOINT_SCAN_BUDGET_BYTES;
        let ownerless_count = targets
            .iter()
            .filter(|target| target.awaiting_owner)
            .count();
        let selected_ownerless = Self::next_ownerless_for_home(home, ownerless_count);
        let mut ownerless_index = 0;
        for target in targets.iter() {
            let selected_for_scan =
                target.awaiting_owner && selected_ownerless == Some(ownerless_index);
            if target.awaiting_owner {
                ownerless_index += 1;
            }
            if !valid_id(&target.id) {
                continue;
            }
            let Some(updated) = updated_at(&target.id)? else {
                continue;
            };
            let metadata_recent = now
                .checked_sub(updated)
                .is_some_and(|age| (0..=switcher::RECENT_QUOTA_WINDOW_SECS).contains(&age));
            if target.awaiting_owner {
                // An unselected retry cannot be dated safely from SQLite.
                // The deferred dispatcher re-prunes it as a singleton before IPC.
                if !selected_for_scan {
                    eligible.push(target.clone());
                    continue;
                }
                let stable = Self::stable_tail_state(home, &target.id, &mut inspect)?;
                let stable_state = stable.as_ref().map(|(state, _, _)| *state);
                let is_recent = if stable_state == Some(ThreadRolloutState::InterruptedByQuota) {
                    let Some((_, path, before)) = stable.as_ref() else {
                        return Err("Quota rollout snapshot disappeared".into());
                    };
                    Self::checked_quota_recency(home, &target.id, now, path, before)?
                        .ok_or("Quota failure timestamp unavailable for deferred recovery")?
                } else {
                    metadata_recent
                };
                if !is_recent {
                    continue;
                }
                if stable_state == Some(ThreadRolloutState::InterruptedByError) {
                    // Unattended recovery cannot continue a non-quota error,
                    // including one with queued work. Inspect only this pass's
                    // selected ownerless target and require a stable file.
                    continue;
                }
                if post_checkpoint_status_with_budget(home, target, &mut scan_budget)
                    .is_some_and(|(_, verified)| verified)
                    && confirmed_checkpoint_status_with_budget(home, target, &mut scan_budget)
                        .is_some_and(|(_, verified)| verified)
                    && pending_count(home, &target.id)? == 0
                    && confirmed_snapshot_still_current(home, target)
                {
                    continue;
                }
                // Apart from a stable terminal non-quota error, tail state
                // cannot retire an undispatched retry. Never inspect an
                // unselected ownerless tail under this bounded prune pass.
                eligible.push(target.clone());
                continue;
            }
            let stable = Self::stable_tail_state(home, &target.id, &mut inspect)?;
            let retain = match stable.as_ref().map(|(state, _, _)| *state) {
                Some(ThreadRolloutState::InterruptedByQuota) => {
                    let Some((_, path, before)) = stable.as_ref() else {
                        return Err("Quota rollout snapshot disappeared".into());
                    };
                    Self::checked_quota_recency(home, &target.id, now, path, before)?
                        .unwrap_or(false)
                }
                Some(ThreadRolloutState::ActiveInProgress | ThreadRolloutState::TurnAborted) => {
                    metadata_recent
                }
                // An error-ended turn may be a policy block. Only an explicit
                // target request may continue it.
                Some(ThreadRolloutState::InterruptedByError) => false,
                Some(ThreadRolloutState::CleanCompleted) => {
                    metadata_recent && pending_count(home, &target.id)? > 0
                }
                Some(ThreadRolloutState::Unknown) | None => false,
            };
            if retain {
                eligible.push(target.clone());
            }
        }
        *targets = eligible;
        Ok(())
    }

    fn stable_tail_state(
        home: &Path,
        id: &str,
        inspect: &mut impl FnMut(&Path, &str) -> ThreadRolloutState,
    ) -> Result<Option<(ThreadRolloutState, PathBuf, std::fs::Metadata)>, String> {
        let Some(path) = switcher::find_thread_rollout_path(home, id) else {
            return Ok(None);
        };
        let Ok(before) = std::fs::symlink_metadata(&path) else {
            return Err("Rollout changed during manifest pruning".into());
        };
        if !before.is_file() || before.file_type().is_symlink() {
            return Ok(None);
        }
        let state = inspect(home, id);
        if !Self::tail_unchanged(home, id, &path, &before) {
            return Err("Rollout changed during manifest pruning".into());
        }
        Ok(Some((state, path, before)))
    }

    fn checked_quota_recency(
        home: &Path,
        id: &str,
        now: i64,
        path: &Path,
        before: &std::fs::Metadata,
    ) -> Result<Option<bool>, String> {
        Self::checked_quota_recency_with(
            home,
            id,
            now,
            path,
            before,
            switcher::quota_failure_timestamp,
        )
    }

    fn checked_quota_recency_with(
        home: &Path,
        id: &str,
        now: i64,
        path: &Path,
        before: &std::fs::Metadata,
        mut failure_timestamp: impl FnMut(&Path, &str) -> Option<i64>,
    ) -> Result<Option<bool>, String> {
        let recent = failure_timestamp(home, id)
            .and_then(|failed_at| now.checked_sub(failed_at))
            .map(|age| (0..=switcher::RECENT_QUOTA_WINDOW_SECS).contains(&age));
        if !Self::tail_unchanged(home, id, path, before) {
            return Err("Rollout changed during manifest pruning".into());
        }
        Ok(recent)
    }

    fn tail_unchanged(home: &Path, id: &str, path: &Path, before: &std::fs::Metadata) -> bool {
        let Ok(after) = std::fs::symlink_metadata(path) else {
            return false;
        };
        after.is_file()
            && !after.file_type().is_symlink()
            && before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && (before.ctime(), before.ctime_nsec()) == (after.ctime(), after.ctime_nsec())
            && switcher::find_thread_rollout_path(home, id).as_deref() == Some(path)
    }

    fn next_ownerless_for_home(home: &Path, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let mut cursors = NEXT_OWNERLESS_SCAN
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !cursors.contains_key(home) && cursors.len() >= MAX_ROTATION_HOMES {
            if let Some(evicted) = cursors.keys().next().cloned() {
                cursors.remove(&evicted);
            }
        }
        let cursor = cursors.entry(home.to_path_buf()).or_default();
        let selected = *cursor % count;
        *cursor = (selected + 1) % count;
        Some(selected)
    }
}

#[cfg(test)]
#[path = "manifest_prune_service.test.rs"]
mod tests;
