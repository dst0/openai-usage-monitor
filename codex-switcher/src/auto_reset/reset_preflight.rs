use crate::models::AccountConfig;

/// Result of the final eligibility checks that must all pass before a reset
/// attempt may be marked `pending`.
pub(super) enum ResetPreflight {
    /// Every check passed; carries the registry's current copy of the account.
    Ready(Box<AccountConfig>),
    /// No request may be sent. `record` is set only when the attempt is
    /// otherwise ready and should show why it waits (Desktop is closed).
    Refused {
        state: String,
        reason: Option<String>,
        record: bool,
    },
}

impl ResetPreflight {
    pub(super) fn refused(state: &str, reason: Option<&str>) -> Self {
        Self::Refused {
            state: state.into(),
            reason: reason.map(Into::into),
            record: false,
        }
    }
}
