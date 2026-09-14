//! Conservative weekly reset-credit automation.
//!
//! A reset credit is spent through the authenticated ChatGPT service used by
//! the quota reader. Desktop remains the sole owner of threads and is used
//! only for post-reset task recovery.

use crate::models::{AccountConfig, Settings};
use crate::{quota, recovery, storage, switcher};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const JOURNAL_VERSION: u8 = 1;
const MAX_THRESHOLD_SECONDS: u64 = 167 * 3600;
static NEXT_IDEMPOTENCY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub(crate) struct AutoResetStatus {
    pub state: String,
    pub reason: Option<String>,
    pub last_event_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AutoResetReport {
    pub status: AutoResetStatus,
    /// A successful or uncertain reset attempt must settle before the normal
    /// account-rotation branch runs; otherwise it could replace the account
    /// while the owner is still applying the reset.
    pub suppress_auto_switch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResetJournal {
    version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    episode_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    idempotency_key: Option<String>,
    #[serde(default)]
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
}

impl Default for ResetJournal {
    fn default() -> Self {
        Self {
            version: JOURNAL_VERSION,
            episode_key: None,
            account_id: None,
            thread_id: None,
            idempotency_key: None,
            state: "ready".into(),
            reason: None,
            updated_at: None,
        }
    }
}

fn journal_path() -> PathBuf {
    storage::codex_home().join("auto-reset-state.json")
}

fn load_journal_at(path: &Path) -> Result<ResetJournal, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ResetJournal::default())
        }
        Err(error) => return Err(format!("Unable to inspect auto-reset journal: {error}")),
    };
    // Reset state controls a consumable account resource. Refuse a symlink,
    // foreign-owned file, or broad permissions instead of trusting it.
    // SAFETY: geteuid has no inputs and does not access Rust-managed memory.
    let expected_uid = unsafe { libc::geteuid() };
    if !metadata.file_type().is_file()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o777 != 0o600
    {
        return Err("Auto-reset journal ownership or permissions are unsafe".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| format!("Unable to open auto-reset journal: {error}"))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|error| format!("Unable to read auto-reset journal: {error}"))?;
    let journal: ResetJournal = serde_json::from_str(&content).map_err(|_| {
        "Auto-reset journal is invalid; refusing to spend a reset credit".to_string()
    })?;
    if journal.version != JOURNAL_VERSION {
        return Err(
            "Auto-reset journal version is unsupported; refusing to spend a reset credit".into(),
        );
    }
    Ok(journal)
}

fn load_journal() -> Result<ResetJournal, String> {
    load_journal_at(&journal_path())
}

/// This is a small, random-access state document rather than a log, so it is
/// atomically replaced and kept uncompressed. Brotli would make each daemon
/// tick needlessly expensive and does not support safe in-place updates.
fn write_journal_at(path: &Path, journal: &ResetJournal) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Auto-reset journal has no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let content = serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?;
    let temp = parent.join(format!(
        ".auto-reset-state.{}.{}.tmp",
        std::process::id(),
        NEXT_IDEMPOTENCY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(&content)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temp, path).map_err(|error| error.to_string())?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_journal(journal: &ResetJournal) -> Result<(), String> {
    write_journal_at(&journal_path(), journal)
}

fn weekly_exhausted(active: &AccountConfig) -> bool {
    // The cached value is derived directly from `(100 - used_percent)` and
    // multiplied afterwards, so exact zero remains exact zero. We deliberately
    // do not use rounded menu-bar percentages here.
    active.last_weekly_percentage == Some(0.0)
}

fn weekly_reset_reflected(usage: &crate::models::WhamUsageResponse) -> Option<bool> {
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

fn episode_key(active: &AccountConfig) -> String {
    let reset_marker = active
        .last_weekly_reset_time
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown-reset-window");
    format!("{}|{reset_marker}", active.account_id)
}

fn same_episode(journal: &ResetJournal, active: &AccountConfig) -> bool {
    journal.episode_key.as_deref() == Some(episode_key(active).as_str())
        && journal.account_id.as_deref() == Some(active.account_id.as_str())
}

fn now_string() -> String {
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

fn report(
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

fn threshold_eligible(active: &AccountConfig, threshold: u64) -> Result<bool, String> {
    if threshold > MAX_THRESHOLD_SECONDS {
        return Err("Weekly reset threshold exceeds 167 hours".into());
    }
    if threshold == 0 {
        return Ok(true);
    }
    let remaining = active.last_weekly_reset_after_seconds.unwrap_or(0);
    Ok(remaining > threshold as i64)
}

fn terminal_no_spend_state(state: &str) -> bool {
    state == "not_consumed"
}

/// Gives the menu a durable, safe-to-display state before the daemon decides
/// whether an eligible task exists. It never performs an IPC request.
pub(crate) fn status_for_active(
    settings: &Settings,
    active: Option<&AccountConfig>,
) -> AutoResetStatus {
    if !settings.auto_reset_weekly_enabled {
        return AutoResetStatus {
            state: "disabled".into(),
            reason: None,
            last_event_at: None,
        };
    }
    let Some(active) = active else {
        return AutoResetStatus {
            state: "waiting_for_active_account".into(),
            reason: None,
            last_event_at: None,
        };
    };
    if !weekly_exhausted(active) {
        return AutoResetStatus {
            state: "ready".into(),
            reason: None,
            last_event_at: None,
        };
    }
    if active.last_error.is_some() {
        return AutoResetStatus {
            state: "waiting_for_fresh_quota".into(),
            reason: Some("active_account_usage_read_failed".into()),
            last_event_at: None,
        };
    }
    if active.last_credits.unwrap_or(0) == 0 {
        return AutoResetStatus {
            state: "no_credit".into(),
            reason: None,
            last_event_at: None,
        };
    }
    if !threshold_eligible(active, settings.auto_reset_weekly_min_remaining_seconds)
        .unwrap_or(false)
    {
        return AutoResetStatus {
            state: "waiting_for_window".into(),
            reason: None,
            last_event_at: None,
        };
    }
    match load_journal() {
        Ok(journal) if same_episode(&journal, active) => AutoResetStatus {
            state: journal.state,
            reason: journal.reason,
            last_event_at: journal.updated_at,
        },
        Ok(_) => AutoResetStatus {
            state: "waiting_for_task".into(),
            reason: None,
            last_event_at: None,
        },
        Err(_) => AutoResetStatus {
            state: "journal_error".into(),
            reason: Some("auto_reset_journal_invalid".into()),
            last_event_at: None,
        },
    }
}

/// Clears a previous episode once usage shows a non-zero weekly pool again.
/// Without this, a provider response lacking a reset timestamp could prevent a
/// future, independent weekly-exhaustion episode from being considered.
pub(crate) fn clear_completed_episode_if_restored(
    settings: &Settings,
    active: Option<&AccountConfig>,
) -> Result<(), String> {
    if !settings.auto_reset_weekly_enabled || active.is_none_or(weekly_exhausted) {
        return Ok(());
    }
    let journal = load_journal()?;
    if journal.episode_key.is_some() {
        write_journal(&ResetJournal::default())?;
    }
    Ok(())
}

/// Applies the account-bound reset policy once. The daemon calls this only
/// after obtaining fresh quota data; any result which could have consumed a
/// credit suppresses account rotation until the next fresh usage snapshot.
pub(crate) fn maybe_consume_weekly_reset(
    settings: &Settings,
    active: &AccountConfig,
) -> Result<AutoResetReport, String> {
    if !settings.auto_reset_weekly_enabled {
        return Ok(report("disabled", None, None, false));
    }
    if !weekly_exhausted(active) {
        clear_completed_episode_if_restored(settings, Some(active))?;
        return Ok(report("ready", None, None, false));
    }
    if active.last_error.is_some() {
        return Ok(report(
            "waiting_for_fresh_quota",
            Some("active_account_usage_read_failed".into()),
            None,
            false,
        ));
    }
    if active.last_credits.unwrap_or(0) == 0 {
        return Ok(report("no_credit", None, None, false));
    }
    if !threshold_eligible(active, settings.auto_reset_weekly_min_remaining_seconds)? {
        return Ok(report("waiting_for_window", None, None, false));
    }

    let mut journal = load_journal()?;
    let journal_matches_episode = same_episode(&journal, active);
    let blocked_threads = switcher::detect_recent_quota_blocked_user_threads();
    let Some(discovered_anchor) = blocked_threads.first().cloned() else {
        if journal_matches_episode {
            if journal.state == "applied" {
                return Ok(report("applied", journal.reason, journal.updated_at, true));
            }
            if terminal_no_spend_state(&journal.state) {
                return Ok(report(
                    journal.state,
                    journal.reason,
                    journal.updated_at,
                    false,
                ));
            }
            let suppress_auto_switch = matches!(journal.state.as_str(), "pending" | "unknown");
            return Ok(report(
                if suppress_auto_switch {
                    "waiting_for_original_task"
                } else {
                    "waiting_for_task"
                },
                journal.reason,
                journal.updated_at,
                suppress_auto_switch,
            ));
        }
        return Ok(report("waiting_for_task", None, None, false));
    };

    if journal_matches_episode {
        if journal.state == "applied" {
            return Ok(report("applied", journal.reason, journal.updated_at, true));
        }
        if terminal_no_spend_state(&journal.state) {
            return Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                false,
            ));
        }
        let Some(original_anchor) = journal.thread_id.clone() else {
            return Ok(report(
                "journal_error",
                Some("auto_reset_journal_missing_anchor".into()),
                journal.updated_at,
                true,
            ));
        };
        if !blocked_threads.contains(&original_anchor) {
            let outcome_may_be_unknown = matches!(journal.state.as_str(), "pending" | "unknown");
            return Ok(report(
                "waiting_for_original_task",
                Some("original_reset_task_is_no_longer_quota_blocked".into()),
                journal.updated_at,
                outcome_may_be_unknown,
            ));
        }
    } else {
        let anchor_thread = discovered_anchor;
        journal = ResetJournal {
            version: JOURNAL_VERSION,
            episode_key: Some(episode_key(active)),
            account_id: Some(active.account_id.clone()),
            thread_id: Some(anchor_thread.clone()),
            idempotency_key: Some(new_idempotency_key()?),
            state: "pending".into(),
            reason: None,
            updated_at: Some(now_string()),
        };
        // Persist before dispatch. A process crash after the service request
        // is sent must retry this exact operation rather than manufacture a
        // new credit use.
        write_journal(&journal)?;
    }

    let idempotency_key = journal
        .idempotency_key
        .clone()
        .ok_or("Auto-reset journal has no idempotency key")?;
    let _operation = match recovery::operation_lock() {
        Ok(lock) => lock,
        Err(error) => {
            return Ok(report(
                "waiting_for_other_automation",
                Some(error),
                journal.updated_at,
                true,
            ))
        }
    };

    // Re-check both the active account and opt-in after acquiring the operation
    // lock. A menu change or account switch while quota was being fetched must
    // never spend a credit for an outdated account.
    let latest = storage::load_accounts()?;
    let still_active = latest.active_account_id.as_deref() == Some(active.id.as_str());
    let still_enabled = latest.settings.auto_reset_weekly_enabled
        && latest.settings.auto_reset_weekly_min_remaining_seconds
            == settings.auto_reset_weekly_min_remaining_seconds;
    if !still_active || !still_enabled {
        return Ok(report(
            "policy_changed",
            Some("active_account_or_auto_reset_policy_changed".into()),
            journal.updated_at,
            false,
        ));
    }
    let Some(latest_active) = latest
        .accounts
        .iter()
        .find(|account| account.id == active.id)
    else {
        return Ok(report(
            "policy_changed",
            Some("active_account_disappeared".into()),
            journal.updated_at,
            false,
        ));
    };
    // Recheck the freshly persisted service snapshot after taking the same
    // operation lock used by account switching. This prevents a stale daemon
    // decision from spending against a recovered or differently-routed account.
    if latest_active.account_id != active.account_id
        || latest_active.last_error.is_some()
        || !weekly_exhausted(latest_active)
    {
        return Ok(report(
            "policy_changed",
            Some("active_account_or_weekly_quota_changed".into()),
            journal.updated_at,
            false,
        ));
    }
    if latest_active.last_credits.unwrap_or(0) == 0 {
        return Ok(report("no_credit", None, journal.updated_at, false));
    }
    if !threshold_eligible(
        latest_active,
        latest.settings.auto_reset_weekly_min_remaining_seconds,
    )? {
        return Ok(report(
            "waiting_for_window",
            None,
            journal.updated_at,
            false,
        ));
    }
    let live_auth_matches = storage::read_active_auth_json()
        .ok()
        .and_then(|auth| auth.tokens)
        .is_some_and(|tokens| {
            tokens.access_token == latest_active.tokens.access_token
                && tokens.account_id.as_deref() == latest_active.tokens.account_id.as_deref()
        });
    if !live_auth_matches {
        return Ok(report(
            "policy_changed",
            Some("active_auth_changed_before_reset".into()),
            journal.updated_at,
            false,
        ));
    }
    if !switcher::is_codex_app_running() {
        journal.state = "waiting_for_desktop".into();
        journal.reason = Some("desktop_not_running".into());
        journal.updated_at = Some(now_string());
        write_journal(&journal)?;
        return Ok(report(
            journal.state,
            journal.reason,
            journal.updated_at,
            false,
        ));
    }

    let outcome = quota::consume_rate_limit_reset_credit(latest_active, &idempotency_key);
    match outcome {
        quota::ResetCreditConsumeOutcome::Applied => {
            journal.state = "applied".into();
            journal.reason = None;
            journal.updated_at = Some(now_string());
            write_journal(&journal)?;
            // The service outcome is authoritative for idempotency, but the
            // official contract requires a fresh limits read afterwards. This
            // read also gives recovery a chance to observe the restored pool.
            let mut refreshed_account = latest_active.clone();
            let quota_refresh_reason = match quota::fetch_account_usage(&mut refreshed_account) {
                Ok(usage)
                    if usage.account_id.as_deref() == Some(latest_active.account_id.as_str())
                        && weekly_reset_reflected(&usage) == Some(true) =>
                {
                    None
                }
                Ok(_) => Some("reset_applied_quota_refresh_unverified".to_string()),
                Err(_) => Some("reset_applied_quota_refresh_failed".to_string()),
            };
            let recovery_reason =
                recovery::recover_threads(&blocked_threads, recovery::RecoveryMode::DiscoveredOnly)
                    .err()
                    .map(|reason| format!("reset_applied_recovery_unverified:{reason}"));
            journal.reason = match (quota_refresh_reason, recovery_reason) {
                (None, None) => None,
                (Some(reason), None) | (None, Some(reason)) => Some(reason),
                (Some(quota_reason), Some(recovery_reason)) => {
                    Some(format!("{quota_reason};{recovery_reason}"))
                }
            };
            if journal.reason.is_some() {
                journal.updated_at = Some(now_string());
                write_journal(&journal)?;
            }
            Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                true,
            ))
        }
        quota::ResetCreditConsumeOutcome::NotConsumed(reason) => {
            journal.state = "not_consumed".into();
            journal.reason = Some(reason);
            journal.updated_at = Some(now_string());
            write_journal(&journal)?;
            Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                false,
            ))
        }
        quota::ResetCreditConsumeOutcome::Unavailable(reason) => {
            journal.state = "waiting_for_service".into();
            journal.reason = Some(reason);
            journal.updated_at = Some(now_string());
            write_journal(&journal)?;
            Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                false,
            ))
        }
        quota::ResetCreditConsumeOutcome::Unknown(reason) => {
            journal.state = "unknown".into();
            journal.reason = Some(reason);
            journal.updated_at = Some(now_string());
            write_journal(&journal)?;
            Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                true,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_is_private_and_atomically_readable() {
        let root = std::env::temp_dir().join(format!(
            "codex-auto-reset-test-{}-{}",
            std::process::id(),
            NEXT_IDEMPOTENCY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("auto-reset-state.json");
        let journal = ResetJournal {
            episode_key: Some("account|window".into()),
            account_id: Some("account".into()),
            thread_id: Some("00000000-0000-0000-0000-000000000000".into()),
            idempotency_key: Some("opaque-key".into()),
            state: "pending".into(),
            updated_at: Some("2026-01-01T00:00:00Z".into()),
            ..ResetJournal::default()
        };
        write_journal_at(&path, &journal).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(load_journal_at(&path).unwrap().state, "pending");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_journal_at(&path).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn threshold_is_strict_when_nonzero() {
        let mut account = test_account();
        account.last_weekly_reset_after_seconds = Some(86_400);
        assert!(!threshold_eligible(&account, 86_400).unwrap());
        account.last_weekly_reset_after_seconds = Some(86_401);
        assert!(threshold_eligible(&account, 86_400).unwrap());
        assert!(threshold_eligible(&account, 0).unwrap());
    }

    #[test]
    fn exact_zero_is_required_for_weekly_exhaustion() {
        let mut account = test_account();
        account.last_weekly_percentage = Some(0.01);
        assert!(!weekly_exhausted(&account));
        account.last_weekly_percentage = Some(0.0);
        assert!(weekly_exhausted(&account));
    }

    #[test]
    fn old_unsupported_state_is_retryable_via_the_service() {
        assert!(!terminal_no_spend_state("unsupported"));
        assert!(terminal_no_spend_state("not_consumed"));
    }

    #[test]
    fn idempotency_keys_are_uuid_v4_values() {
        let key = new_idempotency_key().unwrap();
        assert_eq!(key.len(), 36);
        assert_eq!(&key[14..15], "4");
        assert!(matches!(key.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }

    fn test_account() -> AccountConfig {
        AccountConfig {
            id: "monitor-account".into(),
            name: None,
            email: "user@example.invalid".into(),
            plan_type: "team".into(),
            account_id: "account-id".into(),
            tokens: crate::models::AuthTokens {
                access_token: "not-a-real-token".into(),
                refresh_token: None,
                id_token: None,
                account_id: Some("account-id".into()),
            },
            enabled: true,
            priority: 0,
            last_primary_percentage: 0.0,
            last_reset_time: None,
            last_reset_after_seconds: None,
            last_weekly_percentage: Some(0.0),
            last_weekly_reset_time: Some("2026-01-08T00:00:00Z".into()),
            last_weekly_reset_after_seconds: Some(100_000),
            last_credits: Some(1),
            last_error: None,
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
            organization_name: None,
        }
    }
}
