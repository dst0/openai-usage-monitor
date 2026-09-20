use super::{
    process_identity::ProcessIdentity, recovery_session::RecoverySession, saved_window::SavedWindow,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const BANNER_TITLE: &str = "Codex Monitor • Восстановление задач";
pub const BANNER_EXPLANATION: &str =
    "Codex перезапускается, вернёт окно на прежнее место и продолжит эти задачи:";
pub const MINIMUM_VISIBLE_MS: u64 = 5_000;

/// Versioned private hand-off consumed by the native banner helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryBannerPayload {
    pub version: u32,
    pub operation_id: String,
    pub title: String,
    pub explanation: String,
    pub expected_process: ProcessIdentity,
    pub saved_window: SavedWindow,
    pub sessions: Vec<RecoverySession>,
    pub minimum_visible_ms: u64,
    pub updated_at_unix_ms: u64,
}

impl RecoveryBannerPayload {
    pub fn new(
        operation_id: impl Into<String>,
        expected_process: ProcessIdentity,
        saved_window: SavedWindow,
        sessions: Vec<RecoverySession>,
    ) -> Result<Self, String> {
        let operation_id = operation_id.into();
        if operation_id.trim().is_empty()
            || operation_id.len() > 128
            || operation_id.chars().any(|ch| ch.is_control())
        {
            return Err("Recovery banner operation ID is invalid".into());
        }
        Ok(Self {
            version: 1,
            operation_id,
            title: BANNER_TITLE.to_string(),
            explanation: BANNER_EXPLANATION.to_string(),
            expected_process,
            saved_window,
            sessions,
            minimum_visible_ms: MINIMUM_VISIBLE_MS,
            updated_at_unix_ms: now_unix_ms(),
        })
    }

    pub fn replace_sessions(&mut self, sessions: Vec<RecoverySession>) {
        self.sessions = sessions;
        self.updated_at_unix_ms = now_unix_ms();
    }

    pub fn update_status(
        &mut self,
        short_id: &str,
        status: super::banner_session_status::BannerSessionStatus,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .iter_mut()
            .find(|session| session.short_id == short_id)
            .ok_or_else(|| "Recovery banner target is not present".to_string())?;
        session.status = status;
        self.updated_at_unix_ms = now_unix_ms();
        Ok(())
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
