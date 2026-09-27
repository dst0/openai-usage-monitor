/// Which pass of an explicitly requested window-task restore is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowTaskRestorePhase<'a> {
    /// Rebuilds every captured window in the relaunched Desktop.
    AfterRelaunch,
    /// Moves back only windows that recovery moved onto one of these tasks
    /// with its own task links; the user's later changes stay.
    AfterRecovery(&'a [String]),
}

impl WindowTaskRestorePhase<'_> {
    pub fn label(self) -> &'static str {
        match self {
            Self::AfterRelaunch => "after relaunch",
            Self::AfterRecovery(_) => "after recovery",
        }
    }
}
