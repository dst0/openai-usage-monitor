#[derive(Debug)]
pub(super) enum IpcReadError {
    Io(std::io::Error),
    Protocol(String),
}

impl std::fmt::Display for IpcReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Protocol(error) => formatter.write_str(error),
        }
    }
}
