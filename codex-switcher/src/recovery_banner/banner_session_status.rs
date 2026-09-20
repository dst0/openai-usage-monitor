use serde::{Deserialize, Serialize};

/// The status rendered beside one recovery target in the native banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerSessionStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl BannerSessionStatus {
    pub fn indicator(self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◐",
            Self::Completed => "✓",
            Self::Failed => "✕",
            Self::Skipped => "↷",
        }
    }
}
