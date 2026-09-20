use super::app_lifecycle::AppLifecycle;
use super::desktop_app_session::DesktopAppSession;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_decision_service::DistributionDecisionService;
use super::distribution_outcome::DistributionOutcome;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_service::DistributionTransactionService;
use super::log_redaction_service::LogRedactionService;
use super::system_app_lifecycle::SystemAppLifecycle;
use crate::storage;
use chrono::Utc;
use std::sync::Arc;

pub struct DistributionCoordinator {
    decision_service: DistributionDecisionService,
    transaction_service: DistributionTransactionService,
    logger: DistributionAuditLogger,
    lifecycle: Arc<dyn AppLifecycle>,
}

impl Default for DistributionCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl DistributionCoordinator {
    pub fn new() -> Self {
        Self::with_lifecycle(Arc::new(SystemAppLifecycle::default()))
    }

    pub fn with_lifecycle(lifecycle: Arc<dyn AppLifecycle>) -> Self {
        let logger = DistributionAuditLogger::default();
        Self {
            decision_service: DistributionDecisionService::new(),
            transaction_service: DistributionTransactionService::with_lifecycle(
                logger.clone(),
                lifecycle.clone(),
            ),
            logger,
            lifecycle,
        }
    }

    pub fn execute(&self, request: DistributionRequest) -> Result<DistributionOutcome, String> {
        let op_id = generate_operation_id();
        let trigger_str = request.trigger.as_str();

        self.logger.log_request(
            &op_id,
            trigger_str,
            &request.reason,
            &format!("Evaluating distribution dry_run={}", request.dry_run),
        );

        let accounts_file = match storage::load_accounts() {
            Ok(accounts_file) => accounts_file,
            Err(error) => {
                self.log_failed_outcome(&op_id, trigger_str, &request.reason, "state_load_failed");
                return Err(error);
            }
        };
        if accounts_file.accounts.is_empty() {
            let msg = "No accounts configured in accounts.json";
            self.logger
                .log_warning(&op_id, "DECISION", trigger_str, &request.reason, msg);
            return Ok(DistributionOutcome::no_action(
                &op_id,
                trigger_str,
                &request.reason,
                None,
                None,
                msg,
            ));
        }

        let home = storage::codex_home();

        let existing_journal = match super::distribution_journal::DistributionJournal::load(&home) {
            Ok(journal) => journal,
            Err(error) => {
                self.log_failed_outcome(&op_id, trigger_str, &request.reason, "journal_invalid");
                return Err(error);
            }
        };
        if let Some(existing) = existing_journal {
            if existing.is_stale(std::time::Duration::from_secs(300)) {
                self.logger.log_warning(
                    &op_id,
                    "STALE_JOURNAL",
                    trigger_str,
                    &request.reason,
                    &format!("Cleaning up stale journal from pid={}", existing.pid),
                );
                if let Err(error) = super::distribution_journal::DistributionJournal::clear(&home) {
                    self.log_failed_outcome(
                        &op_id,
                        trigger_str,
                        &request.reason,
                        "stale_journal_cleanup_failed",
                    );
                    return Err(error);
                }
                let _ = crate::recovery::arm_automation_cooldown();
            } else {
                self.logger.log_warning(
                    &op_id,
                    "DEFERRED_IN_FLIGHT",
                    trigger_str,
                    &request.reason,
                    &format!(
                        "Operation {} currently in flight (pid={})",
                        existing.operation_id, existing.pid
                    ),
                );
                return Ok(DistributionOutcome::deferred_in_flight(
                    &op_id,
                    trigger_str,
                    &request.reason,
                    format!("Operation {} currently in flight", existing.operation_id),
                ));
            }
        }

        if request.trigger.is_auto() && !request.force_restart {
            match crate::recovery::automation_cooldown_remaining() {
                Ok(Some(remaining)) => {
                    self.logger.log_action(
                        &op_id,
                        "COOLDOWN_ACTIVE",
                        trigger_str,
                        &request.reason,
                        &format!(
                            "Distribution deferred for {}s during cooldown",
                            remaining.as_secs().saturating_add(1)
                        ),
                    );
                    return Ok(DistributionOutcome::deferred_cooldown(
                        &op_id,
                        trigger_str,
                        &request.reason,
                        format!(
                            "Deferred during automation cooldown ({}s remaining)",
                            remaining.as_secs()
                        ),
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    self.log_failed_outcome(
                        &op_id,
                        trigger_str,
                        &request.reason,
                        "cooldown_state_invalid",
                    );
                    return Err(error);
                }
            }
        }

        let is_desktop_running = self.lifecycle.is_app_running();
        let desktop_session = DesktopAppSession::load(&home.join("desktop-app-session.json"));

        let current_cli_id = accounts_file.active_account_id.as_deref();
        let current_app_id = if is_desktop_running {
            desktop_session
                .as_ref()
                .map(|s| s.account_id.as_str())
                .or(current_cli_id)
        } else {
            desktop_session.as_ref().map(|s| s.account_id.as_str())
        };

        let plan = self.decision_service.evaluate(
            &accounts_file,
            current_app_id,
            current_cli_id,
            is_desktop_running,
            &request,
        );

        let candidate_summary = plan
            .evaluated_candidates
            .iter()
            .map(|c| {
                if c.eligible {
                    format!("{}:eligible", c.sanitized_label)
                } else {
                    format!(
                        "{}:skip({})",
                        c.sanitized_label,
                        c.skip_reason.as_deref().unwrap_or("ineligible")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        self.logger.log_decision(
            &op_id,
            trigger_str,
            &request.reason,
            &format!(
                "app_target={} cli_target={} restart_required={} candidates=[{}]",
                account_ref(plan.target_app_id.as_deref()),
                account_ref(plan.target_cli_id.as_deref()),
                plan.restart_required,
                candidate_summary
            ),
        );

        if !plan.has_changes() && !request.force_restart {
            let msg = format!("No action needed: {}", plan.decision_reason);
            self.logger
                .log_action(&op_id, "NO_ACTION", trigger_str, &request.reason, &msg);
            return Ok(DistributionOutcome::no_action(
                &op_id,
                trigger_str,
                &request.reason,
                plan.current_app_id,
                plan.current_cli_id,
                msg,
            ));
        }

        if request.dry_run {
            let msg = format!(
                "Dry run completed: app_target={}, cli_target={}",
                account_ref(plan.target_app_id.as_deref()),
                account_ref(plan.target_cli_id.as_deref())
            );
            self.logger
                .log_action(&op_id, "DRY_RUN", trigger_str, &request.reason, &msg);
            return Ok(DistributionOutcome::no_action(
                &op_id,
                trigger_str,
                &request.reason,
                plan.target_app_id,
                plan.target_cli_id,
                msg,
            ));
        }

        match self
            .transaction_service
            .execute(&op_id, &plan, &request, accounts_file)
        {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.log_failed_outcome(&op_id, trigger_str, &request.reason, "transaction_failed");
                Err(error)
            }
        }
    }

    fn log_failed_outcome(&self, op_id: &str, trigger: &str, reason: &str, code: &str) {
        self.logger.log_failure(
            op_id,
            "OUTCOME",
            trigger,
            reason,
            &format!("status=failed code={code}"),
        );
    }
}

pub fn generate_operation_id() -> String {
    let mut random_bytes = [0u8; 6];
    let _ = getrandom::getrandom(&mut random_bytes);
    let hex_suffix = hex_encode(&random_bytes);
    format!("op_dist_{}_{}", Utc::now().timestamp_millis(), hex_suffix)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn account_ref(account_id: Option<&str>) -> String {
    account_id
        .map(|value| LogRedactionService::sanitize_field("account_id", value))
        .unwrap_or_else(|| "none".to_string())
}
