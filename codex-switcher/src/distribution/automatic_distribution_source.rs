#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomaticDistributionSource {
    Daemon,
    WrapperPreflight,
}

impl AutomaticDistributionSource {
    pub fn reason(self, cause: &str) -> String {
        match self {
            Self::Daemon => cause.to_string(),
            Self::WrapperPreflight => format!("wrapper_preflight_{cause}"),
        }
    }
}
