#[derive(Debug)]
pub(super) enum DispatchMarkError {
    AccountChanged,
    Other(String),
}

impl std::fmt::Display for DispatchMarkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountChanged => formatter
                .write_str("Active Desktop account or process changed before recovery dispatch"),
            Self::Other(error) => formatter.write_str(error),
        }
    }
}
