use std::fmt;
use std::io;

/// Where replacing a private state file stopped. Every stage before
/// `SyncDirectory` leaves the previous file in place. `SyncDirectory` means the
/// new content is already visible under the final name but may not survive a
/// crash or power loss.
#[derive(Debug)]
pub(crate) enum StateFileWriteFailure {
    PrepareDirectory(io::Error),
    CreateStaging(io::Error),
    SaveStaging(io::Error),
    Replace(io::Error),
    SyncDirectory(io::Error),
}

impl fmt::Display for StateFileWriteFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (stage, error) = match self {
            Self::PrepareDirectory(error) => ("state directory could not be prepared", error),
            Self::CreateStaging(error) => ("staging file could not be created", error),
            Self::SaveStaging(error) => ("staging file could not be saved", error),
            Self::Replace(error) => ("state file could not be replaced", error),
            Self::SyncDirectory(error) => ("state directory could not be synced", error),
        };
        write!(formatter, "{stage}: {error}")
    }
}
