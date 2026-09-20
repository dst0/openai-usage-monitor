#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryMode {
    CapturedRestart,
    ExplicitTarget,
    DiscoveredOnly,
}

impl RecoveryMode {
    pub(super) fn captured(self) -> bool {
        self == Self::CapturedRestart
    }

    pub(super) fn allows_ambiguous_active_dispatch(self) -> bool {
        matches!(self, Self::CapturedRestart | Self::ExplicitTarget)
    }
}
