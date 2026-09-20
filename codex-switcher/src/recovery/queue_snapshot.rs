use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread::sleep,
    time::Duration,
};
const MAX_QUEUE_STATE: usize = 512 * 1024;
const SQLITE_READ_ATTEMPTS: usize = 12;
const SQLITE_BUSY_TIMEOUT_MS: u64 = 10000;
pub(super) const INTERRUPTED_QUEUE_PAUSE: &str = "Interrupted before the steer was accepted.";

pub(super) fn query(database: &Path, sql: &str) -> Result<String, String> {
    let mut last_error = String::new();
    for attempt in 0..SQLITE_READ_ATTEMPTS {
        let output = Command::new("/usr/bin/sqlite3")
            .args([
                "-readonly",
                "-cmd",
                &format!(".timeout {SQLITE_BUSY_TIMEOUT_MS}"),
            ])
            .arg(database)
            .arg(sql)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if attempt + 1 < SQLITE_READ_ATTEMPTS {
            let sleep_ms = 250 + (attempt as u64 * 100);
            sleep(Duration::from_millis(sleep_ms));
        }
    }
    Err(format!(
        "Could not read Codex recovery state from {} after {} retries: {}",
        database
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("db"),
        SQLITE_READ_ATTEMPTS,
        if last_error.is_empty() {
            "timeout or database busy"
        } else {
            &last_error
        }
    ))
}

pub(super) fn pending_count(home: &Path, id: &str) -> Result<usize, String> {
    let db = home.join("queue_1.sqlite");
    if !db.exists() {
        return Ok(0);
    }
    query(
        &db,
        &format!("SELECT count(*) FROM queued_items WHERE thread_id = '{id}';"),
    )?
    .parse()
    .map_err(|_| "Invalid queue count".into())
}

pub(super) fn queue_revision(home: &Path, id: &str) -> Result<u64, String> {
    let db = home.join("queue_1.sqlite");
    if !db.exists() {
        return Ok(0);
    }
    let value = query(
        &db,
        &format!(
            "SELECT COALESCE((SELECT revision FROM queued_thread_revisions WHERE thread_id = '{id}'), 0);"
        ),
    )?;
    value.parse().map_err(|_| "Invalid queue revision".into())
}

pub(super) fn validate_queue_snapshot_revision(before: u64, after: u64) -> Result<(), String> {
    if before == after {
        Ok(())
    } else {
        Err("Codex queue changed while recovery was reading it".into())
    }
}

pub(super) fn parse_queued_rows(bytes: &[u8]) -> Result<Vec<Value>, String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    serde_json::from_slice(bytes).map_err(|_| "Codex queue returned invalid JSON".into())
}

pub(super) fn queued_messages(home: &Path, id: &str) -> Result<Vec<Value>, String> {
    let db = home.join("queue_1.sqlite");
    if !db.exists() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT payload_json FROM queued_items WHERE thread_id = '{id}' ORDER BY queue_order;"
    );
    let mut last_error = String::new();
    for attempt in 0..SQLITE_READ_ATTEMPTS {
        let mut child = Command::new("/usr/bin/sqlite3")
            .args([
                "-readonly",
                "-json",
                "-cmd",
                &format!(".timeout {SQLITE_BUSY_TIMEOUT_MS}"),
            ])
            .arg(&db)
            .arg(&sql)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or("Could not read Codex queue state")?
            .take(MAX_QUEUE_STATE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_QUEUE_STATE {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Codex queue state exceeds the recovery memory limit".into());
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if status.success() {
            let rows = parse_queued_rows(&bytes)?;
            let mut messages = Vec::with_capacity(rows.len());
            for row in rows {
                let payload = row["payload_json"]
                    .as_str()
                    .ok_or("Codex queue row is missing its payload")?;
                let message: Value = serde_json::from_str(payload)
                    .map_err(|_| "Codex queue contains an invalid message")?;
                if !message.is_object() {
                    return Err("Codex queue contains a non-object message".into());
                }
                messages.push(message);
            }
            return Ok(messages);
        }
        if let Some(mut err_pipe) = child.stderr.take() {
            let mut err_buf = Vec::new();
            let _ = err_pipe.read_to_end(&mut err_buf);
            last_error = String::from_utf8_lossy(&err_buf).trim().to_string();
        }
        if attempt + 1 < SQLITE_READ_ATTEMPTS {
            let sleep_ms = 250 + (attempt as u64 * 100);
            sleep(Duration::from_millis(sleep_ms));
        }
    }
    Err(format!(
        "Could not read Codex queued messages after {} retries: {}",
        SQLITE_READ_ATTEMPTS,
        if last_error.is_empty() {
            "timeout or database busy"
        } else {
            &last_error
        }
    ))
}

pub(super) fn prepare_interrupted_queue(messages: &mut [Value]) -> Result<bool, String> {
    if messages.is_empty() {
        return Err("Codex queue changed while recovery was preparing it".into());
    }
    let mut changed = false;
    for message in messages {
        let object = message
            .as_object_mut()
            .ok_or("Codex queue contains a non-object message")?;
        match object.get("pausedReason") {
            Some(Value::String(reason)) if reason == INTERRUPTED_QUEUE_PAUSE => {
                object.remove("pausedReason");
                changed = true;
            }
            Some(Value::Null) | None => {}
            Some(_) => {
                return Err(
                    "Queued work has a non-restart pause reason; refusing to resume it".into(),
                )
            }
        }
    }
    Ok(changed)
}
