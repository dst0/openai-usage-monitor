use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_journal::DistributionJournal;
use super::distribution_outcome::DistributionOutcome;
use std::path::Path;
use std::time::Duration;

/// A dead worker is not proof that its shared-auth write was rolled back.
pub(super) struct DistributionJournalGateService;

impl DistributionJournalGateService {
    pub(super) fn inspect(
        home: &Path,
        logger: &DistributionAuditLogger,
        operation_id: &str,
        trigger: &str,
        reason: &str,
    ) -> Result<Option<DistributionOutcome>, String> {
        let existing = DistributionJournal::load(home).inspect_err(|_| {
            Self::log_failed(logger, operation_id, trigger, reason, "journal_invalid");
        })?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        if !existing.is_stale(Duration::from_secs(300)) {
            logger.log_warning(
                operation_id,
                "DEFERRED_IN_FLIGHT",
                trigger,
                reason,
                &format!(
                    "Operation {} currently in flight (pid={})",
                    existing.operation_id, existing.pid
                ),
            );
            return Ok(Some(DistributionOutcome::deferred_in_flight(
                operation_id,
                trigger,
                reason,
                format!("Operation {} currently in flight", existing.operation_id),
            )));
        }
        if !existing.permits_stale_cleanup() {
            Self::log_failed(
                logger,
                operation_id,
                trigger,
                reason,
                "stale_uncertain_distribution_journal",
            );
            return Err("Prior account distribution may have stopped Desktop or changed credentials; inspect shared authentication, registry, recovery checkpoint, and Desktop session before retrying".into());
        }
        logger.log_warning(
            operation_id,
            "STALE_JOURNAL",
            trigger,
            reason,
            &format!("Cleaning up stale journal from pid={}", existing.pid),
        );
        DistributionJournal::clear(home).inspect_err(|_| {
            Self::log_failed(
                logger,
                operation_id,
                trigger,
                reason,
                "stale_journal_cleanup_failed",
            );
        })?;
        let _ = crate::recovery::arm_automation_cooldown();
        Ok(None)
    }

    fn log_failed(
        logger: &DistributionAuditLogger,
        operation_id: &str,
        trigger: &str,
        reason: &str,
        code: &str,
    ) {
        logger.log_failure(
            operation_id,
            "OUTCOME",
            trigger,
            reason,
            &format!("status=failed code={code}"),
        );
    }
}
