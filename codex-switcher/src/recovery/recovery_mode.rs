#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryMode {
    CapturedRestart,
    DeferredOwned,
    DeferredCaptured,
    ExplicitTarget,
    DiscoveredOnly,
}

impl RecoveryMode {
    pub(super) fn preserves_checkpoint(self) -> bool {
        matches!(
            self,
            Self::CapturedRestart | Self::DeferredOwned | Self::DeferredCaptured
        )
    }

    pub(super) fn allows_ambiguous_active_dispatch(self) -> bool {
        matches!(
            self,
            Self::CapturedRestart | Self::DeferredCaptured | Self::ExplicitTarget
        )
    }
}
