use super::auto_reset_status::AutoResetStatus;

#[derive(Debug, Clone)]
pub(crate) struct AutoResetReport {
    pub status: AutoResetStatus,
    /// A successful or uncertain reset attempt must settle before the normal
    /// account-rotation branch runs; otherwise it could replace the account
    /// while the owner is still applying the reset.
    pub suppress_auto_switch: bool,
}
