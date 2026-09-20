use crate::storage;
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub(crate) const AUTOMATION_COOLDOWN: Duration = Duration::from_secs(30);

pub(crate) fn operation_id_for_banner(reason: &str) -> String {
    if let Ok(operation_id) = std::env::var("CODEX_RESTART_OPERATION") {
        let operation_id = operation_id.trim();
        if !operation_id.is_empty()
            && operation_id.len() <= 128
            && operation_id.chars().all(|ch| !ch.is_control())
        {
            return operation_id.to_string();
        }
    }
    let safe_reason: String = reason
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .take(32)
        .collect();
    let safe_reason = if safe_reason.is_empty() {
        "recovery"
    } else {
        &safe_reason
    };
    format!(
        "op_{safe_reason}_{}_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        std::process::id()
    )
}

pub fn operation_lock() -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(storage::codex_home().join("desktop-recovery.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock_exclusive()
        .map_err(|_| "Another desktop switch/recovery is in progress".to_string())?;
    Ok(file)
}

pub(super) fn cooldown_path(home: &Path) -> PathBuf {
    home.join("desktop-automation-cooldown")
}

pub(super) fn cooldown_deadline_ms(now: SystemTime, duration: Duration) -> Result<u64, String> {
    let deadline = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is before Unix epoch".to_string())?
        .checked_add(duration)
        .ok_or("Automation cooldown deadline overflow")?;
    u64::try_from(deadline.as_millis()).map_err(|_| "Automation cooldown deadline overflow".into())
}

pub(super) fn arm_automation_cooldown_at(
    home: &Path,
    now: SystemTime,
    duration: Duration,
) -> Result<(), String> {
    std::fs::create_dir_all(home).map_err(|error| error.to_string())?;
    let deadline = cooldown_deadline_ms(now, duration)?;
    let unique = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = home.join(format!(
        "desktop-automation-cooldown.{}.{unique}.tmp",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        writeln!(file, "{deadline}").map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, cooldown_path(home)).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Suppresses every automatic account switch for a short, durable settling
/// period. This tiny atomically-replaced state is intentionally not Brotli-
/// compressed because it needs cheap random reads on every daemon tick.
pub(crate) fn arm_automation_cooldown() -> Result<(), String> {
    arm_automation_cooldown_at(
        &storage::codex_home(),
        SystemTime::now(),
        AUTOMATION_COOLDOWN,
    )
}

pub(super) fn automation_cooldown_remaining_at(
    home: &Path,
    now: SystemTime,
) -> Result<Option<Duration>, String> {
    let value = match std::fs::read_to_string(cooldown_path(home)) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let deadline_ms = value
        .trim()
        .parse::<u64>()
        .map_err(|_| "Invalid desktop automation cooldown state".to_string())?;
    let now_ms = u64::try_from(
        now.duration_since(UNIX_EPOCH)
            .map_err(|_| "System clock is before Unix epoch".to_string())?
            .as_millis(),
    )
    .map_err(|_| "System clock overflow".to_string())?;
    Ok((deadline_ms > now_ms).then(|| Duration::from_millis(deadline_ms - now_ms)))
}

pub(crate) fn automation_cooldown_remaining() -> Result<Option<Duration>, String> {
    automation_cooldown_remaining_at(&storage::codex_home(), SystemTime::now())
}

pub(super) fn restart_cancellation_path() -> PathBuf {
    storage::codex_home()
        .join("recovery-runs")
        .join("cancel-restart")
}

pub(crate) fn restart_cancellation_requested() -> bool {
    restart_cancellation_path().is_file()
}

pub(crate) fn clear_restart_cancellation() -> Result<(), String> {
    match std::fs::remove_file(restart_cancellation_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn valid_operation_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn claim_restart_operation_at(home: &Path, operation_id: &str) -> Result<bool, String> {
    if !valid_operation_id(operation_id) {
        return Err("Invalid restart operation ID".into());
    }
    let directory = home.join("recovery-runs");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("restart-{operation_id}.claimed"));
    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(b"claimed\n")
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Destructive restart workers are at-most-once. `launchctl submit` keeps a
/// failed job alive, so an operation tombstone must be claimed before the app
/// is stopped. A relaunched worker sees the claim and exits without restarting.
pub fn claim_restart_operation() -> Result<bool, String> {
    let operation_id = std::env::var("CODEX_RESTART_OPERATION")
        .map_err(|_| "Restart worker is missing its operation ID".to_string())?;
    claim_restart_operation_at(&storage::codex_home(), &operation_id)
}
