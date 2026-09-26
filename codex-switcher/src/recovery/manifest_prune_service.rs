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
            let is_recent = updated_at(&target.id)?
                .is_some_and(|updated| (now - updated).abs() <= switcher::RECENT_QUOTA_WINDOW_SECS);
            if !is_recent {
                continue;
            }
            if target.awaiting_owner {
                if selected_for_scan && Self::stable_error_ended_tail(home, &target.id, &mut inspect)
                {
                    // Unattended recovery cannot continue a non-quota error,
                    // including one with queued work. Inspect only this pass's
                    // selected ownerless target and require a stable file.
                    continue;
                }
                if selected_for_scan
                    && post_checkpoint_status_with_budget(home, target, &mut scan_budget)
                        .is_some_and(|(_, verified)| verified)
                    && confirmed_checkpoint_status_with_budget(home, target, &mut scan_budget)
                        .is_some_and(|(_, verified)| verified)
                    && pending_count(home, &target.id)? == 0
                    && confirmed_snapshot_still_current(home, target)
                {
                    continue;
                }
                // No tail classification can retire an undispatched retry.
                // Inspecting every ownerless tail would defeat the one-target
                // bounded scan even when the cached cursor stays untouched.
                eligible.push(target.clone());
                continue;
            }
            let retain = match inspect(home, &target.id) {
                ThreadRolloutState::ActiveInProgress
                | ThreadRolloutState::InterruptedByQuota
                | ThreadRolloutState::TurnAborted => true,
                // An error-ended turn may be a policy block. Only an explicit
                // target request may continue it.
                ThreadRolloutState::InterruptedByError => false,
                ThreadRolloutState::CleanCompleted => pending_count(home, &target.id)? > 0,
                ThreadRolloutState::Unknown => false,
            };
            if retain {
                eligible.push(target.clone());
            }
        }
        *targets = eligible;
        Ok(())
    }

    fn stable_error_ended_tail(
        home: &Path,
        id: &str,
        inspect: &mut impl FnMut(&Path, &str) -> ThreadRolloutState,
    ) -> bool {
        let Some(path) = switcher::find_thread_rollout_path(home, id) else {
            return false;
        };
        let Ok(before) = std::fs::symlink_metadata(&path) else {
            return false;
        };
        if !before.is_file() || before.file_type().is_symlink() {
            return false;
        }
        let state = inspect(home, id);
        let Ok(after) = std::fs::symlink_metadata(&path) else {
            return false;
        };
        state == ThreadRolloutState::InterruptedByError
            && after.is_file()
            && before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && (before.ctime(), before.ctime_nsec()) == (after.ctime(), after.ctime_nsec())
            && switcher::find_thread_rollout_path(home, id).as_deref() == Some(path.as_path())
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
