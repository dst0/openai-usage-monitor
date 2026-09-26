use super::{append_eligible_pending, detect_quota_blocked_user_threads_since_at};
use chrono::{TimeZone, Utc};
use std::process::Command;

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
