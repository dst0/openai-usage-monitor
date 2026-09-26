pub(super) struct DistributionDesktopSwitchOutcome {
    pub(super) restarted_desktop: bool,
    pub(super) recovery_error: Option<String>,
    pub(super) commit_verified: bool,
}
