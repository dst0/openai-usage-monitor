use super::{
    auth_rotation_queue_snapshot::AuthRotationQueueSnapshot,
    auth_rotation_recovery_evidence::AuthRotationRecoveryEvidence,
    manifest_store::{load_manifest, write_manifest},
    restart_checkpoint_service::save_pending,
};
use crate::switcher::{self, ThreadRolloutState};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

const AUTH_ROTATION_MESSAGE: &str = "Your access token could not be refreshed because you have since logged out or signed in to another account. Please sign in again.";
const MAX_INTERVAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 512 * 1024;
const PRE_STOP_TAIL_BYTES: u64 = 512 * 1024;

pub(crate) struct AuthRotationCheckpointService;

impl AuthRotationCheckpointService {
    /// Capture queue state before the first rollout offset is journaled, so
    /// input arriving during checkpoint preparation cannot become the baseline.
    pub(crate) fn queue_snapshots(
        home: &Path,
        ids: &[String],
    ) -> Result<HashMap<String, AuthRotationQueueSnapshot>, String> {
        ids.iter()
            .map(|id| AuthRotationQueueSnapshot::read(home, id).map(|state| (id.clone(), state)))
            .collect()
    }

    /// Called after the first durable recovery checkpoint, while the verified
    /// source Desktop still runs. An old error is never eligible here.
    pub(crate) fn prepare(
        home: &Path,
        ids: &[String],
        source_account_id: &str,
        target_account_id: &str,
        initial_snapshots: &HashMap<String, AuthRotationQueueSnapshot>,
    ) -> Result<(), String> {
        if source_account_id.is_empty()
            || target_account_id.is_empty()
            || source_account_id == target_account_id
        {
            return Ok(());
        }
        let mut manifest = load_manifest()?;
        for target in manifest
            .iter_mut()
            .filter(|target| ids.contains(&target.id))
        {
            if target.awaiting_owner
                || !target.captured_restart
                || switcher::inspect_thread_rollout_state(home, &target.id)
                    != ThreadRolloutState::ActiveInProgress
            {
                continue;
            }
            let Some(pre_stop_offset) = target.offset else {
                continue;
            };
            let Some(path) = switcher::find_thread_rollout_path(home, &target.id) else {
                continue;
            };
            let Ok(before) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !before.is_file()
                || before.file_type().is_symlink()
                || before.len() != pre_stop_offset
            {
                continue;
            }
            let Some(turn_id) = Self::active_turn_id(home, &target.id) else {
                continue;
            };
            let Ok(after) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !Self::same_snapshot(&before, &after) {
                continue;
            }
            let Some(initial_snapshot) = initial_snapshots.get(&target.id) else {
                continue;
            };
            if AuthRotationQueueSnapshot::read(home, &target.id)? != *initial_snapshot {
                continue;
            }
            target.auth_rotation = Some(AuthRotationRecoveryEvidence {
                source_account_id: source_account_id.to_owned(),
                target_account_id: target_account_id.to_owned(),
                pre_stop_offset,
                rollout_dev: before.dev(),
                rollout_ino: before.ino(),
                turn_id,
                queue_snapshot: initial_snapshot.clone(),
                confirmed_after_stop: false,
            });
        }
        write_manifest(&manifest)
    }

    /// The ordinary second checkpoint excludes shutdown flushes from recovery
    /// proof. Before it replaces the first one, retain its operation evidence
    /// in memory and confirm only the exact auth failure in that interval.
    pub(crate) fn finalize_after_stop(home: &Path, ids: &[String]) -> Result<(), String> {
        let before = load_manifest()?;
        save_pending(ids)?;
        let mut after = load_manifest()?;
        for target in after.iter_mut().filter(|target| ids.contains(&target.id)) {
            let Some(mut evidence) = before
                .iter()
                .find(|prior| prior.id == target.id)
                .and_then(|prior| prior.auth_rotation.clone())
            else {
                continue;
            };
            let Some(final_offset) = target.offset else {
                continue;
            };
            if Self::interval_confirmed(
                home,
                &target.id,
                evidence.pre_stop_offset,
                final_offset,
                &evidence.turn_id,
                evidence.rollout_dev,
                evidence.rollout_ino,
            ) && AuthRotationQueueSnapshot::read(home, &target.id)? == evidence.queue_snapshot
                && switcher::inspect_thread_rollout_state(home, &target.id)
                    == ThreadRolloutState::InterruptedByError
            {
                evidence.confirmed_after_stop = true;
                target.auth_rotation = Some(evidence);
            }
        }
        write_manifest(&after)
    }

    fn active_turn_id(home: &Path, id: &str) -> Option<String> {
        let path = switcher::find_thread_rollout_path(home, id)?;
        let lines = switcher::read_rollout_tail_lines(&path, PRE_STOP_TAIL_BYTES);
        let mut active = None;
        for line in lines {
            let value: Value = serde_json::from_str(&line).ok()?;
            if value["type"] != "event_msg" {
                continue;
            }
            match value["payload"]["type"].as_str() {
                Some("task_started") => {
                    active = value["payload"]["turn_id"].as_str().map(str::to_owned);
                }
                Some("turn_aborted" | "task_complete") => active = None,
                _ => {}
            }
        }
        active
    }

    pub(super) fn interval_confirmed(
        home: &Path,
        id: &str,
        start: u64,
        end: u64,
        turn_id: &str,
        dev: u64,
        ino: u64,
    ) -> bool {
        Self::check_interval(home, id, start, end, turn_id, dev, ino).unwrap_or(false)
    }

    fn check_interval(
        home: &Path,
        id: &str,
        start: u64,
        end: u64,
        turn_id: &str,
        dev: u64,
        ino: u64,
    ) -> Option<bool> {
        let len = end.checked_sub(start)?;
        if len == 0 || len > MAX_INTERVAL_BYTES {
            return None;
        }
        let path = switcher::find_thread_rollout_path(home, id)?;
        let named = fs::symlink_metadata(&path).ok()?;
        if !named.is_file()
            || named.file_type().is_symlink()
            || named.len() != end
            || named.dev() != dev
            || named.ino() != ino
        {
            return None;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .ok()?;
        let opened = file.metadata().ok()?;
        if opened.dev() != named.dev() || opened.ino() != named.ino() || opened.len() != end {
            return None;
        }
        file.seek(SeekFrom::Start(start)).ok()?;
        let matches = Self::stream_matches(file.take(len), len, turn_id);
        let after = fs::symlink_metadata(&path).ok()?;
        Self::same_snapshot(&named, &after).then_some(matches)
    }

    fn same_snapshot(before: &fs::Metadata, after: &fs::Metadata) -> bool {
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && (before.ctime(), before.ctime_nsec()) == (after.ctime(), after.ctime_nsec())
    }

    #[cfg(test)]
    pub(super) fn interval_matches(bytes: &[u8], turn_id: &str) -> bool {
        Self::stream_matches(bytes, bytes.len() as u64, turn_id)
    }

    fn stream_matches(mut reader: impl Read, expected: u64, turn_id: &str) -> bool {
        if expected == 0 || expected > MAX_INTERVAL_BYTES || turn_id.is_empty() {
            return false;
        }
        let mut chunk = [0_u8; 8192];
        let mut line = Vec::with_capacity(4096);
        let mut total = 0_u64;
        let mut terminal = false;
        loop {
            let Ok(read) = reader.read(&mut chunk) else {
                return false;
            };
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > expected {
                return false;
            }
            for byte in &chunk[..read] {
                if *byte == b'\n' {
                    if !Self::record_matches(&line, turn_id, &mut terminal) {
                        return false;
                    }
                    line.clear();
                } else {
                    if line.len() >= MAX_RECORD_BYTES {
                        return false;
                    }
                    line.push(*byte);
                }
            }
        }
        total == expected && line.is_empty() && terminal
    }

    fn record_matches(line: &[u8], turn_id: &str, terminal: &mut bool) -> bool {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            return false;
        };
        if value["type"] == "response_item" {
            return !*terminal
                && !(value["payload"]["type"] == "message" && value["payload"]["role"] == "user");
        }
        if value["type"] != "event_msg" {
            return !*terminal;
        }
        let payload = &value["payload"];
        match payload["type"].as_str() {
            Some("task_started" | "turn_aborted" | "user_message") => false,
            Some("message") if payload["role"] == "user" => false,
            Some("task_complete") => {
                if *terminal
                    || payload["turn_id"].as_str() != Some(turn_id)
                    || payload["error"]["message"].as_str() != Some(AUTH_ROTATION_MESSAGE)
                    || payload["last_agent_message"]
                        .as_str()
                        .is_some_and(|text| !text.trim().is_empty())
                    || (!payload["last_agent_message"].is_null()
                        && !payload["last_agent_message"].is_string())
                {
                    return false;
                }
                *terminal = true;
                true
            }
            Some("token_count" | "thread_settings_applied" | "item_completed") => true,
            Some(_) => !*terminal,
            None => false,
        }
    }
}
