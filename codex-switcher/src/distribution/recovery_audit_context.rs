use super::distribution_request::DistributionRequest;
use super::window_capture_mode::WindowCaptureMode;

/// Immutable inputs for Desktop window restoration and owner-routed recovery.
pub(super) struct RecoveryAuditContext<'a> {
    pub(super) pid: u32,
    pub(super) targets: &'a [String],
    pub(super) capture_mode: WindowCaptureMode,
    pub(super) operation_id: &'a str,
    pub(super) request: &'a DistributionRequest,
}
