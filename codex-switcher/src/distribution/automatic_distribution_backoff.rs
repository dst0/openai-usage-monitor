use super::automatic_distribution_backoff_record::AutomaticDistributionBackoffRecord;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_outcome::DistributionOutcome;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_error::DistributionTransactionError;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Identical consecutive pre-signal failures of one plan before it is held.
const FAILURES_BEFORE_BACKOFF: u32 = 2;
/// Hold after the second identical failure; each further one doubles it.
const BASE_DELAY: Duration = Duration::from_secs(5 * 60);
/// A held plan is retried at least this often.
const MAX_DELAY: Duration = Duration::from_secs(30 * 60);
/// A failure this long after the previous one starts a new streak. A held
/// plan is retried within `MAX_DELAY` plus one tick, which stays inside it.
const STREAK_RESET_AFTER: Duration = Duration::from_secs(60 * 60);

/// Holds back an automatic distribution whose unchanged plan keeps failing
/// the same way before Desktop is signalled. Each such attempt costs a thread
/// and rollout scan plus writer-lock probes, and the cause (for example a
/// denied Accessibility read in the LaunchAgent) rarely clears by itself
/// within one tick. The daemon loop owns one instance in memory, so a daemon
/// restart retries at once. Manual requests neither consult nor change it.
/// It keeps only the latest plan's streak: another plan's failure replaces
/// it. `Instant` stops while the Mac sleeps, so a hold does too.
#[derive(Default)]
pub struct AutomaticDistributionBackoff {
    record: Mutex<Option<AutomaticDistributionBackoffRecord>>,
}

impl AutomaticDistributionBackoff {
    /// The plan identity: the same cause moving the same accounts.
    pub(super) fn key(request: &DistributionRequest, plan: &DistributionPlan) -> String {
        [
            Some(request.reason.as_str()),
            plan.current_app_id.as_deref(),
            plan.current_cli_id.as_deref(),
            plan.target_app_id.as_deref(),
            plan.target_cli_id.as_deref(),
        ]
        .map(|part| part.unwrap_or(""))
        .join("\0")
    }

    /// A deferred outcome while this automatic plan is held back.
    pub(super) fn defer(
        &self,
        logger: &DistributionAuditLogger,
        op_id: &str,
        request: &DistributionRequest,
        plan: &DistributionPlan,
    ) -> Option<DistributionOutcome> {
        self.defer_at(logger, op_id, request, plan, Instant::now())
    }

    /// Logs `AUTO_BACKOFF_ACTIVE` once per hold; the daemon's watchdog can
    /// wake it every two seconds while the active account is depleted.
    pub(super) fn defer_at(
        &self,
        logger: &DistributionAuditLogger,
        op_id: &str,
        request: &DistributionRequest,
        plan: &DistributionPlan,
        now: Instant,
    ) -> Option<DistributionOutcome> {
        if !request.trigger.is_auto() {
            return None;
        }
        let key = Self::key(request, plan);
        let (remaining, first_notice) = {
            let mut record = self.lock();
            let held = record.as_mut().filter(|record| record.key == key)?;
            let remaining = held.until.filter(|until| *until > now)? - now;
            (remaining, !std::mem::replace(&mut held.announced, true))
        };
        let seconds = remaining.as_secs().saturating_add(1);
        let trigger = request.trigger.as_str();
        if first_notice {
            logger.log_warning(
                op_id,
                "AUTO_BACKOFF_ACTIVE",
                trigger,
                &request.reason,
                &format!(
                    "Automatic distribution held for {seconds}s after repeated identical pre-signal failures"
                ),
            );
        }
        Some(DistributionOutcome::deferred_cooldown(
            op_id,
            trigger,
            &request.reason,
            format!("Deferred after repeated identical pre-signal failures ({seconds}s remaining)"),
        ))
    }

    /// Counts an automatic pre-signal failure; any other automatic result
    /// ends the streak.
    pub(super) fn observe(
        &self,
        logger: &DistributionAuditLogger,
        op_id: &str,
        request: &DistributionRequest,
        plan: &DistributionPlan,
        error: Option<&DistributionTransactionError>,
    ) {
        if !request.trigger.is_auto() {
            return;
        }
        let Some((error, phase)) =
            error.and_then(|error| error.pre_signal_phase().map(|phase| (error, phase)))
        else {
            self.clear();
            return;
        };
        let key = Self::key(request, plan);
        let Some((failures, delay)) =
            self.record_pre_signal_failure(&key, phase, error.message(), Instant::now())
        else {
            return;
        };
        logger.log_warning(
            op_id,
            "AUTO_BACKOFF_ARMED",
            request.trigger.as_str(),
            &request.reason,
            &format!(
                "failures={failures} failed_phase={phase} Automatic distribution paused for {}s; manual switches are not affected",
                delay.as_secs()
            ),
        );
    }

    #[cfg(test)]
    pub(crate) fn remaining(&self, key: &str, now: Instant) -> Option<Duration> {
        let record = self.lock();
        let until = record.as_ref().filter(|record| record.key == key)?.until?;
        (until > now).then(|| until - now)
    }

    /// A held automatic plan waits for the configured daemon interval. The
    /// lightweight deferred-recovery poll continues during that interval.
    pub(super) fn has_active_hold(&self, now: Instant) -> bool {
        self.lock()
            .as_ref()
            .and_then(|record| record.until)
            .is_some_and(|until| until > now)
    }

    /// Returns the failure count and hold when this failure arms a backoff.
    pub(crate) fn record_pre_signal_failure(
        &self,
        key: &str,
        phase: &'static str,
        message: &str,
        now: Instant,
    ) -> Option<(u32, Duration)> {
        let mut record = self.lock();
        let failures = match record.as_ref() {
            Some(previous)
                if previous.key == key
                    && previous.phase == phase
                    && previous.message == message
                    && now.saturating_duration_since(previous.last_failure)
                        <= STREAK_RESET_AFTER =>
            {
                previous.failures.saturating_add(1)
            }
            _ => 1,
        };
        let delay = (failures >= FAILURES_BEFORE_BACKOFF).then(|| Self::delay(failures));
        *record = Some(AutomaticDistributionBackoffRecord {
            key: key.to_string(),
            phase,
            message: message.to_string(),
            failures,
            last_failure: now,
            until: delay.map(|delay| now + delay),
            announced: false,
        });
        delay.map(|delay| (failures, delay))
    }

    pub(crate) fn clear(&self) {
        *self.lock() = None;
    }

    fn delay(failures: u32) -> Duration {
        let doublings = (failures - FAILURES_BEFORE_BACKOFF).min(16);
        BASE_DELAY.saturating_mul(1 << doublings).min(MAX_DELAY)
    }

    /// A panic while holding the lock leaves a complete record behind.
    fn lock(&self) -> MutexGuard<'_, Option<AutomaticDistributionBackoffRecord>> {
        self.record
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
#[path = "automatic_distribution_backoff.test.rs"]
mod tests;
