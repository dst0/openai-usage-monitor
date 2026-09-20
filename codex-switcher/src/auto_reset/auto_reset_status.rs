#[derive(Debug, Clone)]
pub(crate) struct AutoResetStatus {
    pub state: String,
    pub reason: Option<String>,
    pub last_event_at: Option<String>,
}
