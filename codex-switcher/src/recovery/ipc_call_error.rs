#[derive(Debug)]
pub(super) enum IpcCallError {
    NoClientFound,
    Other(String),
}

impl std::fmt::Display for IpcCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoClientFound => formatter.write_str("no-client-found"),
            Self::Other(error) => formatter.write_str(error),
        }
    }
}
