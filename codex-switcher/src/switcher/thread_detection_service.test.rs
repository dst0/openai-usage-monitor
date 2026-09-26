use super::{
    append_eligible_pending, detect_in_progress_threads, detect_quota_blocked_user_threads_since_at,
};
use crate::storage::test_codex_home::TestCodexHome;
use chrono::{TimeZone, Utc};
use fs2::FileExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn reopening_old_quota_failure_does_not_make_it_recent() {
    let root = std::env::temp_dir().join(format!(
        "codex-old-quota-reopen-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let thread_id = "01a0d8a1-f49c-7cd1-8e5f-cbf327837b5d";
    let now = Utc::now().timestamp();
    let old_event = Utc.timestamp_opt(now - 5 * 3600, 0).single().unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-27T00-00-00-{thread_id}.jsonl"));
    let event = serde_json::json!({
        "timestamp": old_event.to_rfc3339(),
        "type": "event_msg",
        "payload": {"type": "task_complete", "turn_id": "old-turn", "last_agent_message": null,
            "error": {"message": "usage_limit_exceeded"}}
    });
    let reopened = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(), "type": "event_msg",
        "payload": {"type": "thread_settings_applied"}
    });
    std::fs::write(&rollout, format!("{event}\n{reopened}\n")).unwrap();
    let db = root.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT); INSERT INTO threads VALUES ('{thread_id}', 0, 'user', {now}, '{}');",
        rollout.display()
    );
    let status = Command::new("/usr/bin/sqlite3")
        .arg(db)
        .arg(sql)
        .status()
        .unwrap();
    assert!(status.success());

    let mut detected = Vec::new();
    append_eligible_pending(&root, &mut detected, vec![thread_id.to_string()]);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        detected.is_empty(),
        "opening an old task cannot refresh its quota failure"
    );
}

#[test]
fn quota_discovery_uses_terminal_event_time_and_fails_closed() {
    let root = std::env::temp_dir().join(format!(
        "codex-quota-event-time-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let now = Utc
        .with_ymd_and_hms(2026, 9, 27, 12, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    let ids = [
        "01a0d8a1-f49c-7cd1-8e5f-cbf327837001", // old failure, recent SQLite row
        "01a0d8a1-f49c-7cd1-8e5f-cbf327837002", // recent failure, old SQLite row
        "01a0d8a1-f49c-7cd1-8e5f-cbf327837003", // missing timestamp
        "01a0d8a1-f49c-7cd1-8e5f-cbf327837004", // malformed timestamp
        "01a0d8a1-f49c-7cd1-8e5f-cbf327837005", // future timestamp
        "01a0d8a1-f49c-7cd1-8e5f-cbf327837006", // exact 4-hour boundary
    ];
    let times = [
        Some(
            Utc.timestamp_opt(now - 5 * 3600, 0)
                .single()
                .unwrap()
                .to_rfc3339(),
        ),
        Some(
            Utc.timestamp_opt(now - 30, 0)
                .single()
                .unwrap()
                .to_rfc3339(),
        ),
        None,
        Some("not a date".to_string()),
        Some(
            Utc.timestamp_opt(now + 30, 0)
                .single()
                .unwrap()
                .to_rfc3339(),
        ),
        Some(
            Utc.timestamp_opt(now - 4 * 3600, 0)
                .single()
                .unwrap()
                .to_rfc3339(),
        ),
    ];
    let mut sql = String::from("CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);");
    for (index, (id, timestamp)) in ids.iter().zip(times).enumerate() {
        let rollout = sessions.join(format!("rollout-2026-09-27T00-00-00-{id}.jsonl"));
        let mut event = serde_json::json!({
            "timestamp": timestamp, "type": "event_msg",
            "payload": {"type": "task_complete", "turn_id": "quota-turn",
                "last_agent_message": null, "error": {"message": "usage_limit_exceeded"}}
        });
        if timestamp.is_none() {
            event.as_object_mut().unwrap().remove("timestamp");
        }
        let reopened = serde_json::json!({
            "timestamp": Utc.timestamp_opt(now, 0).single().unwrap().to_rfc3339(),
            "type": "event_msg", "payload": {"type": "thread_settings_applied"}
        });
        std::fs::write(&rollout, format!("{event}\n{reopened}\n")).unwrap();
        let updated_at = if index == 1 { now - 10 * 86400 } else { now };
        sql.push_str(&format!(
            "INSERT INTO threads VALUES ('{id}', 0, 'user', {updated_at}, '{}');",
            rollout.display()
        ));
    }
    let status = Command::new("/usr/bin/sqlite3")
        .arg(root.join("state_5.sqlite"))
        .arg(sql)
        .status()
        .unwrap();
    assert!(status.success());

    let detected = detect_quota_blocked_user_threads_since_at(&root, now, 4 * 3600);
    let rapid_probe = detect_quota_blocked_user_threads_since_at(&root, now, 30);
    let expired_probe = detect_quota_blocked_user_threads_since_at(&root, now, 29);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        detected.len(),
        2,
        "only recent quota events qualify: {detected:?}"
    );
    assert!(detected.contains(&ids[1].to_string()));
    assert!(detected.contains(&ids[5].to_string()));
    assert_eq!(rapid_probe, vec![ids[1]]);
    assert!(expired_probe.is_empty());
}

const ACTIVE_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e501";
/// Its lock file exists, but no writer holds it.
const ACTIVE_FREE: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e502";
/// Found by both phases; it must be reported once.
const QUOTA_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e503";
/// The app-server released and removed its lock after the quota error, so
/// only the SQLite phase can find it.
const QUOTA_RELEASED: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e504";
const STALE_QUOTA_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e505";
const COMPLETED_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e506";
const ERROR_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e507";
const ABORTED_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e508";
const SUBAGENT_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e509";
const ARCHIVED_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e50a";
/// Stopped by our restart; only the recovery journal can add it.
const RESTART_ABORTED: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e50b";
/// No `updated_at`: only the rollout's failure time dates it, and both
/// phases find it, so it must still be reported once.
const QUOTA_HELD_UNINDEXED: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e50c";
/// Stale and unlocked: the SQLite phase's own window must exclude it.
const STALE_QUOTA_RELEASED: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e50d";
/// Held, with a fresh `updated_at` from Desktop reopening the task, but its
/// quota failure is five hours old. The lock phase must date the failure,
/// not the row.
const REOPENED_QUOTA_HELD: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e50e";

const ACTIVE: &[&str] = &[
    r#"{"type":"event_msg","payload":{"type":"user_message","message":"Build feature"}}"#,
    r#"{"type":"response_item","payload":{"type":"function_call","name":"exec"}}"#,
];
/// `{failed_at}` is replaced with the fixture's quota failure time.
const QUOTA: &[&str] = &[
    r#"{"type":"event_msg","payload":{"type":"user_message","message":"Run tests"}}"#,
    r#"{"timestamp":"{failed_at}","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":{"message":"You have hit your usage limit","codex_error_info":"usage_limit_exceeded"}}}"#,
    r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"t1"}}"#,
];
const COMPLETED: &[&str] = &[
    r#"{"type":"event_msg","payload":{"type":"user_message","message":"Audit"}}"#,
    r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":null}}"#,
];
// A non-quota failure ends the turn; unattended detection must not replay it.
const ERROR: &[&str] = &[
    r#"{"type":"event_msg","payload":{"type":"user_message","message":"Deploy"}}"#,
    r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":{"message":"stream disconnected before completion","codex_error_info":"internal_server_error"}}}"#,
];
const ABORTED: &[&str] = &[
    r#"{"type":"event_msg","payload":{"type":"user_message","message":"Investigate"}}"#,
    r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#,
];

/// (id, archived, thread_source, `updated_at` age in seconds or `None` for
/// no value, quota failure age in seconds, rollout events). Quota recency
/// comes from the failure time, never from `updated_at`.
type Fixture = (
    &'static str,
    u8,
    &'static str,
    Option<i64>,
    i64,
    &'static [&'static str],
);

const FIXTURES: &[Fixture] = &[
    (ACTIVE_HELD, 0, "user", Some(60), 60, ACTIVE),
    (ACTIVE_FREE, 0, "user", Some(60), 60, ACTIVE),
    (QUOTA_HELD, 0, "user", Some(120), 120, QUOTA),
    (QUOTA_RELEASED, 0, "user", Some(180), 180, QUOTA),
    (STALE_QUOTA_HELD, 0, "user", Some(5 * 3600), 5 * 3600, QUOTA),
    (COMPLETED_HELD, 0, "user", Some(60), 60, COMPLETED),
    (ERROR_HELD, 0, "user", Some(60), 60, ERROR),
    (ABORTED_HELD, 0, "user", Some(60), 60, ABORTED),
    (SUBAGENT_HELD, 0, "subagent", Some(60), 60, QUOTA),
    (ARCHIVED_HELD, 1, "user", Some(60), 60, QUOTA),
    (RESTART_ABORTED, 0, "user", Some(60), 60, ABORTED),
    (QUOTA_HELD_UNINDEXED, 0, "user", None, 90, QUOTA),
    (
        STALE_QUOTA_RELEASED,
        0,
        "user",
        Some(24 * 3600),
        24 * 3600,
        QUOTA,
    ),
    (REOPENED_QUOTA_HELD, 0, "user", Some(30), 5 * 3600, QUOTA),
];

/// A running writer holds each of these through its own open file
/// description. `ACTIVE_FREE` has an unheld lock file; the remaining threads
/// have none.
const HELD_LOCKS: &[&str] = &[
    ACTIVE_HELD,
    QUOTA_HELD,
    STALE_QUOTA_HELD,
    COMPLETED_HELD,
    ERROR_HELD,
    ABORTED_HELD,
    SUBAGENT_HELD,
    ARCHIVED_HELD,
    QUOTA_HELD_UNINDEXED,
    REOPENED_QUOTA_HELD,
];

fn seed_codex_home(home: &Path) {
    let sessions = home.join("sessions/2026/09/26");
    fs::create_dir_all(&sessions).unwrap();
    fs::create_dir_all(home.join("thread-writer-locks")).unwrap();
    let now = Utc::now().timestamp();
    let mut sql = String::from(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);",
    );
    for (id, archived, source, age, failed_age, events) in FIXTURES {
        let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
        let failed_at = Utc
            .timestamp_opt(now - failed_age, 0)
            .single()
            .unwrap()
            .to_rfc3339();
        let lines = events.join("\n").replace("{failed_at}", &failed_at);
        fs::write(&rollout, lines + "\n").unwrap();
        let updated_at = age.map_or_else(|| "NULL".to_string(), |age| (now - age).to_string());
        sql.push_str(&format!(
            "INSERT INTO threads VALUES ('{id}', {archived}, '{source}', {updated_at}, '{}');",
            rollout.display().to_string().replace('\'', "''")
        ));
    }
    let status = Command::new("/usr/bin/sqlite3")
        .arg(home.join("state_5.sqlite"))
        .arg(sql)
        .status()
        .unwrap();
    assert!(status.success());
}

fn lock_path(home: &Path, id: &str) -> PathBuf {
    home.join("thread-writer-locks").join(format!("{id}.lock"))
}

/// flock(2) conflicts across separate open file descriptions even within one
/// process, so a fresh descriptor observes what the detector would.
fn is_lock_held(home: &Path, id: &str) -> bool {
    let probe = fs::File::open(lock_path(home, id)).unwrap();
    let held = probe.try_lock_exclusive().is_err();
    if !held {
        probe.unlock().unwrap();
    }
    held
}

struct Detection {
    /// Sorted, with duplicates kept so a double report stays visible.
    detected: Vec<String>,
    free_lock_left_unheld: bool,
    held_locks_kept: bool,
}

/// Runs detection against a freshly seeded private home, optionally with a
/// recovery journal. The guard stays alive until the lock checks have run, so
/// every probe and check sees this test's home and never the live one.
fn detect_in_seeded_home(journal: Option<&str>) -> Detection {
    let guard = TestCodexHome::new("detect-in-progress");
    let home = guard.path();
    seed_codex_home(home);
    if let Some(journal) = journal {
        fs::write(home.join("desktop-recovery.json"), journal).unwrap();
    }
    fs::write(lock_path(home, ACTIVE_FREE), b"").unwrap();
    let held: Vec<fs::File> = HELD_LOCKS
        .iter()
        .map(|id| {
            let file = fs::File::create(lock_path(home, id)).unwrap();
            file.lock_exclusive().unwrap();
            file
        })
        .collect();

    let mut detected = detect_in_progress_threads();

    let free_lock_left_unheld = !is_lock_held(home, ACTIVE_FREE);
    let held_locks_kept = HELD_LOCKS.iter().all(|id| is_lock_held(home, id));
    drop(held);
    drop(guard);
    detected.sort();
    Detection {
        detected,
        free_lock_left_unheld,
        held_locks_kept,
    }
}

#[test]
fn detects_only_live_or_recent_quota_user_threads_in_an_isolated_home() {
    let result = detect_in_seeded_home(None);

    assert!(
        result.free_lock_left_unheld,
        "detection must release a lock it probed"
    );
    assert!(
        result.held_locks_kept,
        "detection must not disturb held locks"
    );
    // Excluded: an active turn without a running writer, stale quota in
    // either phase, an old quota failure whose row a reopen refreshed, clean
    // completion, a non-quota error, a turn_aborted without a journal, a
    // subagent, and an archived thread.
    assert_eq!(
        result.detected,
        [
            ACTIVE_HELD,
            QUOTA_HELD,
            QUOTA_RELEASED,
            QUOTA_HELD_UNINDEXED
        ]
    );
}

#[test]
fn restart_journal_adds_captured_targets_and_withholds_ownerless_ones() {
    // Only the journal distinguishes our restart from a user's Stop. A
    // captured target that detection already found is not added twice. An
    // ownerless retry belongs to the deferred worker, so both phases withhold
    // it: ACTIVE_HELD from the lock phase, QUOTA_RELEASED from the SQLite one.
    let journal = serde_json::json!({
        "version": 1,
        "targets": [
            {"id": RESTART_ABORTED, "offset": null, "captured_restart": true},
            {"id": QUOTA_HELD, "offset": null, "captured_restart": true},
            {"id": ACTIVE_HELD, "offset": null, "awaiting_owner": true,
             "captured_restart": true, "owner_account_id": "owner@example.test:acct"},
            {"id": QUOTA_RELEASED, "offset": 0, "awaiting_owner": true,
             "captured_restart": true, "owner_account_id": "owner@example.test:acct"},
        ],
    });

    let result = detect_in_seeded_home(Some(&journal.to_string()));

    assert!(result.free_lock_left_unheld);
    assert!(result.held_locks_kept);
    assert_eq!(
        result.detected,
        [QUOTA_HELD, RESTART_ABORTED, QUOTA_HELD_UNINDEXED]
    );
}

#[test]
fn unreadable_recovery_journal_blocks_all_detection() {
    // An identically seeded home without a journal reports threads, so the
    // empty result below comes from the journal alone. A journal that cannot
    // be parsed might hold an ownerless retry bound to another account, so
    // detection reports nothing rather than guess.
    assert!(!detect_in_seeded_home(None).detected.is_empty());

    let result = detect_in_seeded_home(Some("{not json"));

    assert!(
        result.detected.is_empty(),
        "a broken journal must not let a restart revive ownerless targets"
    );
}
