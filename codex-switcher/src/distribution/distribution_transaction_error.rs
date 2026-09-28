use std::fmt;

/// A failed distribution transaction. A pre-signal failure names the audit
/// phase that stopped it before Desktop was signalled or credentials changed;
/// only such failures may hold later automatic attempts back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionTransactionError {
    message: String,
    pre_signal_phase: Option<&'static str>,
}

impl DistributionTransactionError {
    pub(super) fn pre_signal(phase: &'static str, message: String) -> Self {
        Self {
            message,
            pre_signal_phase: Some(phase),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn pre_signal_phase(&self) -> Option<&'static str> {
        self.pre_signal_phase
    }

    pub fn into_message(self) -> String {
        self.message
    }
}

impl From<String> for DistributionTransactionError {
    fn from(message: String) -> Self {
        Self {
            message,
            pre_signal_phase: None,
        }
    }
}

impl From<&str> for DistributionTransactionError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl fmt::Display for DistributionTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
