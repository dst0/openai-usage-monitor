use super::reset_journal::ResetJournal;
use super::{AutoResetReport, AutoResetStatus};
use crate::models::AccountConfig;
use chrono::Utc;

const MAX_THRESHOLD_SECONDS: u64 = 167 * 3600;

pub(super) fn weekly_exhausted(active: &AccountConfig) -> bool {
    // The cached value is derived directly from `(100 - used_percent)` and
    // multiplied afterwards, so exact zero remains exact zero. We deliberately
    // do not use rounded menu-bar percentages here.
    active.last_weekly_percentage == Some(0.0)
}

pub(super) fn weekly_reset_reflected(usage: &crate::models::WhamUsageResponse) -> Option<bool> {
    let limits = usage.rate_limit.as_ref()?;
    if let Some(weekly) = limits.secondary_window.as_ref() {
        return Some((100.0 - weekly.used_percent).clamp(0.0, 100.0) > 0.0);
    }
    let weekly = limits
        .primary_window
        .as_ref()
        .filter(|window| window.limit_window_seconds > 86_400)?;
    Some((100.0 - weekly.used_percent).clamp(0.0, 100.0) > 0.0)
}

pub(super) fn episode_key(active: &AccountConfig) -> String {
    let reset_marker = active
        .last_weekly_reset_time
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown-reset-window");
    format!("{}|{reset_marker}", active.account_id)
}

pub(super) fn same_episode(journal: &ResetJournal, active: &AccountConfig) -> bool {
    journal.episode_key.as_deref() == Some(episode_key(active).as_str())
        && journal.account_id.as_deref() == Some(active.account_id.as_str())
}

pub(super) fn unresolved_for_route(journal: &ResetJournal, active: &AccountConfig) -> bool {
    journal.account_id.as_deref() == Some(active.account_id.as_str()) && unresolved_attempt(journal)
}

pub(super) fn unresolved_attempt(journal: &ResetJournal) -> bool {
    matches!(journal.state.as_str(), "pending" | "unknown")
}

pub(super) fn now_string() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn new_idempotency_key() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| format!("Unable to generate reset idempotency key: {error}"))?;
    // RFC 4122 version 4 / variant 1. The key is opaque and never printed;
    // keeping a standard UUID also satisfies Desktop handlers that validate
    // caller-supplied idempotency keys before they forward a reset request.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    ))
}

pub(super) fn report(
    state: impl Into<String>,
    reason: Option<String>,
    last_event_at: Option<String>,
    suppress_auto_switch: bool,
) -> AutoResetReport {
    AutoResetReport {
        status: AutoResetStatus {
            state: state.into(),
            reason,
            last_event_at,
        },
        suppress_auto_switch,
    }
}

pub(super) fn threshold_eligible(active: &AccountConfig, threshold: u64) -> Result<bool, String> {
    if threshold > MAX_THRESHOLD_SECONDS {
        return Err("Weekly reset threshold exceeds 167 hours".into());
    }
    if threshold == 0 {
        return Ok(true);
    }
    let remaining = active.last_weekly_reset_after_seconds.unwrap_or(0);
    Ok(remaining > threshold as i64)
}

pub(super) fn terminal_no_spend_state(state: &str) -> bool {
    state == "not_consumed"
}
