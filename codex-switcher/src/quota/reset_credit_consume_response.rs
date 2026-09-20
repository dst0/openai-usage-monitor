use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ResetCreditConsumeResponse {
    pub(super) code: String,
    #[serde(default)]
    pub(super) windows_reset: i64,
}
