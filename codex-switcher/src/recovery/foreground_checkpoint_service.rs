use super::{
    recovery_service::mark_pre_dispatch_channel_failure,
    recovery_target::{
        record_target_state_with_budget, RecoveryTarget, FOREGROUND_SCAN_BUDGET_BYTES,
    },
};
use crate::switcher::ThreadRolloutState;
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

const PRE_DISPATCH_ACTIVITY_GRACE: Duration = Duration::from_secs(3);
const PRE_DISPATCH_SCAN_TIMEOUT: Duration = Duration::from_secs(90);

/// Completes a bounded, fair scan of every captured checkpoint before IPC can
/// dispatch. An incomplete tail or an active writer that outruns the scanner
/// retains the original checkpoint for a later owner-verified attempt.
pub(super) struct ForegroundCheckpointService<'a> {
    targets: &'a mut Vec<RecoveryTarget>,
}

impl<'a> ForegroundCheckpointService<'a> {
    pub(super) fn new(targets: &'a mut Vec<RecoveryTarget>) -> Self {
        Self { targets }
    }

    pub(super) fn scan_until_ready(&mut self) -> Vec<String> {
        self.scan_until_ready_with(Instant::now, |duration, _| sleep(duration))
    }

    pub(super) fn scan_until_ready_with(
        &mut self,
        mut now: impl FnMut() -> Instant,
        mut wait: impl FnMut(Duration, &[RecoveryTarget]),
    ) -> Vec<String> {
        self.scan_loop(&mut now, &mut wait)
    }

    fn scan_loop(
        &mut self,
        now: &mut impl FnMut() -> Instant,
        wait: &mut impl FnMut(Duration, &[RecoveryTarget]),
    ) -> Vec<String> {
        let started = now();
        let activity_deadline = started + PRE_DISPATCH_ACTIVITY_GRACE;
        let scan_deadline = started + PRE_DISPATCH_SCAN_TIMEOUT;
        loop {
            let active = self
                .targets
                .iter()
                .filter(|target| !target.completed && target.failure.is_none())
                .count();
            if active == 0
                || (now() >= activity_deadline
                    && self
                        .targets
                        .iter()
                        .all(|target| target.scan_complete || target.failure.is_some()))
            {
                break;
            }
            let budget = FOREGROUND_SCAN_BUDGET_BYTES / active as u64;
            for target in &mut *self.targets {
                if !target.completed && target.failure.is_none() {
                    if let Err(error) = record_target_state_with_budget(target, budget) {
                        mark_pre_dispatch_channel_failure(target, &error);
                    } else if !target.scan_complete && now() >= scan_deadline {
                        mark_pre_dispatch_channel_failure(
                            target,
                            "Rollout checkpoint could not be fully checked before dispatch",
                        );
                    }
                }
            }
            wait(Duration::from_millis(200), self.targets.as_slice());
        }
        let mut skipped = Vec::new();
        self.targets.retain(|target| {
            if target.scan_complete
                && target.state == ThreadRolloutState::CleanCompleted
                && !target.completed
                && target.existing_queue == 0
            {
                skipped.push(target.id.clone());
                false
            } else {
                true
            }
        });
        skipped
    }
}
