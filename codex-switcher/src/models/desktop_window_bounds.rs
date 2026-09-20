use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DesktopWindowBounds {
    #[serde(default = "default_version_1")]
    pub version: u8,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub updated_at: u64,
}

fn default_version_1() -> u8 {
    1
}
