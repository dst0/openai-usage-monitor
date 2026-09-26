pub mod app_lifecycle;
pub mod app_stop_error;
pub mod automatic_distribution_service;
pub mod automatic_distribution_source;
mod cli_auth_file_identity_service;
pub mod daemon_account_sync_service;
pub mod daemon_loop_service;
pub mod daemon_tick_service;
pub mod desktop_app_session;
pub(crate) mod desktop_session_verification_service;
pub mod distribution_account_commit_service;
pub mod distribution_audit_logger;
pub mod distribution_candidate;
pub mod distribution_checkpoint_service;
pub mod distribution_coordinator;
pub mod distribution_decision_service;
mod distribution_desktop_auth_handoff_service;
mod distribution_desktop_relaunch_service;
mod distribution_desktop_rollback_service;
mod distribution_desktop_switch_outcome;
mod distribution_desktop_switch_service;
pub mod distribution_executor;
pub mod distribution_journal;
mod distribution_journal_gate_service;
mod distribution_offline_commit_service;
mod distribution_offline_registry_service;
pub mod distribution_outcome;
pub mod distribution_plan;
pub mod distribution_recovery_audit_service;
mod distribution_recovery_preflight_service;
pub mod distribution_request;
mod distribution_shared_auth_guard;
mod distribution_state_preflight_service;
pub mod distribution_transaction_service;
pub mod distribution_trigger;
pub mod historical_log_redaction_service;
mod historical_log_stream_redactor;
pub mod log_permissions_service;
mod log_redaction_output;
pub mod log_redaction_service;
mod log_redaction_span_service;
mod log_redaction_structured_parser;
mod log_redaction_token_service;
#[cfg(test)]
#[path = "mock_app_lifecycle.test.rs"]
pub mod mock_app_lifecycle;
pub mod monitor_log_cleanup_service;
mod monitor_log_directory_reader;
pub mod monitor_log_io_service;
pub(crate) mod monitor_log_lifecycle_lock;
mod monitor_log_name_policy;
mod recovery_audit_context;
pub mod system_app_lifecycle;
pub mod system_window_restore_backend;
mod temporary_log_rewrite;
pub mod window_capture_mode;
pub mod window_process_validation_service;
pub mod window_relaunch_restore_service;
pub mod window_restore_backend;
pub mod window_restore_capture;
pub mod window_restore_capture_result;
pub mod window_restore_event;
pub mod window_restore_frame;
pub mod window_restore_outcome;
pub mod window_restore_process_identity;
pub mod window_restore_report;
pub mod window_restore_sanitizer;
pub mod window_restore_screen;
pub mod window_restore_service;
pub mod window_restore_tolerance;

pub use app_lifecycle::AppLifecycle;
pub use app_stop_error::AppStopError;
pub use automatic_distribution_service::AutomaticDistributionService;
pub use automatic_distribution_source::AutomaticDistributionSource;
pub use desktop_app_session::DesktopAppSession;
pub use distribution_account_commit_service::DistributionAccountCommitService;
pub use distribution_audit_logger::DistributionAuditLogger;
pub use distribution_candidate::DistributionCandidate;
pub use distribution_checkpoint_service::DistributionCheckpointService;
pub use distribution_coordinator::DistributionCoordinator;
pub use distribution_decision_service::DistributionDecisionService;
pub use distribution_executor::DistributionExecutor;
pub use distribution_journal::DistributionJournal;
pub use distribution_outcome::{DistributionOutcome, DistributionStatus};
pub use distribution_plan::DistributionPlan;
pub use distribution_recovery_audit_service::DistributionRecoveryAuditService;
pub use distribution_request::DistributionRequest;
pub use distribution_transaction_service::DistributionTransactionService;
pub use distribution_trigger::DistributionTrigger;
pub use historical_log_redaction_service::HistoricalLogRedactionService;
pub use log_permissions_service::LogPermissionsService;
pub use log_redaction_service::LogRedactionService;
#[cfg(test)]
pub use mock_app_lifecycle::MockAppLifecycle;
pub use monitor_log_cleanup_service::MonitorLogCleanupService;
pub use monitor_log_io_service::MonitorLogIoService;
pub use system_app_lifecycle::SystemAppLifecycle;
pub use system_window_restore_backend::SystemWindowRestoreBackend;
pub use window_capture_mode::WindowCaptureMode;
pub use window_process_validation_service::WindowProcessValidationService;
pub use window_relaunch_restore_service::WindowRelaunchRestoreService;
pub use window_restore_backend::WindowRestoreBackend;
pub use window_restore_capture::WindowCapture;
pub use window_restore_frame::WindowFrame;
pub use window_restore_outcome::RestoreOutcome;
pub use window_restore_process_identity::ProcessIdentity as WindowProcessIdentity;
pub use window_restore_report::RestoreReport;
pub use window_restore_screen::ScreenIdentity;
pub use window_restore_service::WindowRestoreService;
pub use window_restore_tolerance::RestoreTolerance;

#[cfg(test)]
pub mod test_helper;

#[cfg(test)]
#[path = "distribution.test.rs"]
mod tests;

#[cfg(test)]
#[path = "distribution_direct_switch_gate.test.rs"]
mod distribution_direct_switch_gate_tests;

#[cfg(test)]
#[path = "distribution_checkpoint_service.test.rs"]
mod distribution_checkpoint_service_tests;

#[cfg(test)]
#[path = "distribution_desktop_switch_service.test.rs"]
mod distribution_desktop_switch_service_tests;

#[cfg(test)]
#[path = "distribution_transaction_safety.test.rs"]
mod distribution_transaction_safety_tests;

#[cfg(test)]
#[path = "distribution_account_identity_safety.test.rs"]
mod distribution_account_identity_safety_tests;

#[cfg(test)]
#[path = "distribution_identity.test.rs"]
mod identity_tests;

#[cfg(test)]
#[path = "automatic_distribution_service.test.rs"]
mod automatic_distribution_service_tests;

#[cfg(test)]
#[path = "log_redaction_service.test.rs"]
mod log_redaction_service_tests;

#[cfg(test)]
#[path = "historical_log_redaction_service.test.rs"]
mod historical_log_redaction_service_tests;

#[cfg(test)]
#[path = "monitor_log_io_service.test.rs"]
mod monitor_log_io_service_tests;

#[cfg(test)]
#[path = "monitor_log_name_policy.test.rs"]
mod monitor_log_name_policy_tests;

#[cfg(test)]
#[path = "monitor_log_lifecycle_lock.test.rs"]
mod monitor_log_lifecycle_lock_tests;
