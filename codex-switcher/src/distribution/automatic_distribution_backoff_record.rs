use std::time::Instant;

/// The latest run of identical pre-signal failures for one automatic plan.
pub(super) struct AutomaticDistributionBackoffRecord {
    pub(super) key: String,
    pub(super) phase: &'static str,
    pub(super) message: String,
    pub(super) failures: u32,
    pub(super) last_failure: Instant,
    pub(super) until: Option<Instant>,
    /// Whether this hold has already been logged as active.
    pub(super) announced: bool,
}
