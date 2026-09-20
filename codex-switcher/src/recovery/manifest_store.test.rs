use super::{
    manifest_store::{load_manifest, load_pending, write_manifest},
    pending_target::PendingTarget,
    stored_manifest::StoredManifest,
};
use std::{path::PathBuf, process::Command};
struct TestCodexHomeGuard {
    path: PathBuf,
}

impl Drop for TestCodexHomeGuard {
    fn drop(&mut self) {
        std::env::remove_var("CODEX_HOME");
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn legacy_pending_manifest_remains_readable() {
    let json = r#"["01a098c2-0fae-74d2-a80c-45d89e910e79"]"#;
    let stored: StoredManifest = serde_json::from_str(json).unwrap();
    let StoredManifest::Legacy(ids) = stored else {
        panic!("legacy manifest was not recognized")
    };
    assert_eq!(ids, ["01a098c2-0fae-74d2-a80c-45d89e910e79"]);
}

#[test]
fn manifest_cleared_when_targets_empty() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir =
        std::env::temp_dir().join(format!("codex-manifest-clear-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::env::set_var("CODEX_HOME", &temp_dir);
    let _guard = TestCodexHomeGuard {
        path: temp_dir.clone(),
    };

    let targets = vec![PendingTarget {
        id: "01a07d3c-3008-75c2-87a6-2c5c75f0e401".to_string(),
        offset: Some(123),
    }];
    assert!(write_manifest(&targets).is_ok());
    let manifest_path = temp_dir.join("desktop-recovery.json");
    assert!(manifest_path.exists());
    let loaded = load_manifest().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "01a07d3c-3008-75c2-87a6-2c5c75f0e401");

    // When write_manifest is called with empty targets, the file should be deleted
    assert!(write_manifest(&[]).is_ok());
    assert!(!manifest_path.exists());
    let reloaded = load_manifest().unwrap();
    assert!(reloaded.is_empty());
}

#[test]
fn test_load_pending_expunges_stale_targets_from_disk() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir =
        std::env::temp_dir().join(format!("codex-manifest-prune-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::env::set_var("CODEX_HOME", &temp_dir);
    let _guard = TestCodexHomeGuard {
        path: temp_dir.clone(),
    };

    let stale_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e401";
    let completed_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e402";
    let active_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e403";

    let now = chrono::Utc::now().timestamp();
    let old_time = now - 4 * 86400; // 4 days ago
    let fresh_time = now - 300; // 5 minutes ago

    let database = temp_dir.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);\
         INSERT INTO threads VALUES ('{stale_id}', 0, 'user', {old_time}, '');\
         INSERT INTO threads VALUES ('{completed_id}', 0, 'user', {fresh_time}, '');\
         INSERT INTO threads VALUES ('{active_id}', 0, 'user', {fresh_time}, '');"
    );
    let result = Command::new("/usr/bin/sqlite3")
        .arg(&database)
        .arg(sql)
        .status()
        .unwrap();
    assert!(result.success());

    let sessions = temp_dir.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();

    let create_rollout = |tid: &str, line: &str| {
        let path = sessions.join(format!("rollout-2026-09-19T00-00-00-{tid}.jsonl"));
        std::fs::write(&path, format!("{line}\n")).unwrap();
    };

    create_rollout(
        stale_id,
        r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
    );
    create_rollout(
        completed_id,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":null}}"#,
    );
    create_rollout(
        active_id,
        r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
    );

    // Pre-populate manifest with all three targets
    let initial_targets = vec![
        PendingTarget {
            id: stale_id.to_string(),
            offset: Some(10),
        },
        PendingTarget {
            id: completed_id.to_string(),
            offset: Some(20),
        },
        PendingTarget {
            id: active_id.to_string(),
            offset: Some(30),
        },
    ];
    assert!(write_manifest(&initial_targets).is_ok());

    // Calling load_pending must prune stale and completed targets AND persist the cleaned list to disk
    let pending = load_pending().unwrap();
    assert_eq!(pending, vec![active_id.to_string()]);

    // Verify disk state: desktop-recovery.json on disk now only contains active_id
    let reloaded = load_manifest().unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, active_id);

    // If active_id completes and targets becomes empty, write_manifest removes the file completely
    assert!(write_manifest(&[]).is_ok());
    assert!(!temp_dir.join("desktop-recovery.json").exists());
    assert!(load_pending().unwrap().is_empty());
}
