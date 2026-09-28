use super::distribution_transaction_error::DistributionTransactionError;

/// A distribution step that failed before Desktop was signalled, with the
/// audit phase that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionPreSignalFailure {
    pub phase: &'static str,
    pub message: String,
}

impl From<DistributionPreSignalFailure> for DistributionTransactionError {
    fn from(failure: DistributionPreSignalFailure) -> Self {
        Self::pre_signal(failure.phase, failure.message)
    }
}
