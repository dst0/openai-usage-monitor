#[derive(Debug)]
pub struct AppStopError {
    message: String,
    pub before_signal: bool,
}

impl AppStopError {
    pub fn before(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            before_signal: true,
        }
    }

    pub fn after(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            before_signal: false,
        }
    }
}

impl std::fmt::Display for AppStopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
