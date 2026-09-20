use super::{LogRedactionService, MonitorLogIoService};
use chrono::Utc;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DistributionAuditLogger {
    log_file: PathBuf,
}

impl Default for DistributionAuditLogger {
    fn default() -> Self {
        Self::new(crate::logger::switcher_log_path())
    }
}

impl DistributionAuditLogger {
    pub fn new(log_file: impl Into<PathBuf>) -> Self {
        Self {
            log_file: log_file.into(),
        }
    }

    pub fn log_event(
        &self,
        level: &str,
        op_id: &str,
        phase: &str,
        trigger: &str,
        reason: &str,
        message: &str,
    ) {
        let timestamp = Utc::now().to_rfc3339();
        let entry = format!(
            "{timestamp} [{}] [AUDIT] op_id={} phase={} trigger={} reason={} {}\n",
            LogRedactionService::sanitize_text(level),
            LogRedactionService::sanitize_field("op_id", op_id),
            LogRedactionService::sanitize_field("phase", phase),
            LogRedactionService::sanitize_field("trigger", trigger),
            LogRedactionService::sanitize_field("reason", reason),
            LogRedactionService::sanitize_text(message),
        );
        self.write_entry(&entry);
    }

    pub fn log_request(&self, op_id: &str, trigger: &str, reason: &str, details: &str) {
        self.log_event("INFO", op_id, "REQUEST", trigger, reason, details);
    }

    pub fn log_decision(&self, op_id: &str, trigger: &str, reason: &str, details: &str) {
        self.log_event("INFO", op_id, "DECISION", trigger, reason, details);
    }

    pub fn log_lock(&self, op_id: &str, trigger: &str, reason: &str, details: &str) {
        self.log_event("INFO", op_id, "LOCK", trigger, reason, details);
    }

    pub fn log_action(&self, op_id: &str, phase: &str, trigger: &str, reason: &str, details: &str) {
        self.log_event("INFO", op_id, phase, trigger, reason, details);
    }

    pub fn log_warning(
        &self,
        op_id: &str,
        phase: &str,
        trigger: &str,
        reason: &str,
        details: &str,
    ) {
        self.log_event("WARN", op_id, phase, trigger, reason, details);
    }

    pub fn log_failure(
        &self,
        op_id: &str,
        phase: &str,
        trigger: &str,
        reason: &str,
        details: &str,
    ) {
        self.log_event("ERROR", op_id, phase, trigger, reason, details);
    }

    fn write_entry(&self, entry: &str) {
        let _ = MonitorLogIoService::append(&self.log_file, entry.as_bytes());
    }
}
