use super::{
    manifest_store::{
        finalize_target, load_manifest, load_ownerless_pending, load_pending,
        mark_dispatch_attempt_for_account, prune_ineligible_targets_with, save_pending,
        validate_target_account_binding, write_manifest,
    },
    pending_target::PendingTarget,
    stored_manifest::StoredManifest,
};
use std::{path::PathBuf, process::Command};
struct TestCodexHomeGuard {
    path: PathBuf,
}

#[test]
fn unreadable_thread_index_does_not_erase_deferred_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-index-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let target = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let mut pending = vec![target.clone()];
    assert!(prune_ineligible_targets_with(&home, &mut pending, |_| {
        Err("SQLite temporarily unavailable".into())
    })
    .is_err());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].offset, Some(42));
    assert_eq!(pending[0].owner_account_id.as_deref(), Some("account-a"));

    // An unknown rollout is retained only for an already journaled retry;
    // it is never sufficient evidence to create a new recovery request.
    let now = chrono::Utc::now().timestamp();
    prune_ineligible_targets_with(&home, &mut pending, |_| Ok(Some(now))).unwrap();
    assert_eq!(pending.len(), 1);
    pending[0].awaiting_owner = false;
    prune_ineligible_targets_with(&home, &mut pending, |_| Ok(Some(now))).unwrap();
    assert!(pending.is_empty());
    let mut archived = vec![target];
    prune_ineligible_targets_with(&home, &mut archived, |_| Ok(None)).unwrap();
    assert!(archived.is_empty());
    std::fs::remove_dir_all(home).unwrap();
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
fn ownerless_retry_survives_journal_round_trip_without_enabling_old_entries() {
    let old =
        r#"{"version":1,"targets":[{"id":"01a098c2-0fae-74d2-a80c-45d89e910e79","offset":42}]}"#;
    let StoredManifest::Current(old_manifest) =
        serde_json::from_str::<StoredManifest>(old).unwrap()
    else {
        panic!("current manifest was not recognized")
    };
    assert!(!old_manifest.targets[0].awaiting_owner);

    let mut pending = old_manifest.targets;
    pending[0].awaiting_owner = true;
    let encoded = serde_json::to_vec(&super::pending_manifest::PendingManifest {
        version: 1,
        targets: pending,
    })
    .unwrap();
    let StoredManifest::Current(reloaded) = serde_json::from_slice(&encoded).unwrap() else {
        panic!("ownerless manifest was not recognized")
    };
    assert!(reloaded.targets[0].awaiting_owner);
    assert_eq!(reloaded.targets[0].offset, Some(42));
}

#[test]
fn only_failed_pre_dispatch_owner_resolution_keeps_a_retry_checkpoint() {
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let entry = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    };
    let mut no_owner = vec![entry.clone()];
    finalize_target(&mut no_owner, id, true, false, Some("account-a"), false);
    assert!(no_owner[0].awaiting_owner);
    assert_eq!(no_owner[0].owner_account_id.as_deref(), Some("account-a"));
    assert_eq!(no_owner[0].offset, Some(42));

    let mut unknown_ipc_outcome = vec![entry.clone()];
    finalize_target(
        &mut unknown_ipc_outcome,
        id,
        true,
        true,
        Some("account-a"),
        false,
    );
    assert!(unknown_ipc_outcome.is_empty());

    let mut unrelated_failure = vec![entry];
    finalize_target(
        &mut unrelated_failure,
        id,
        false,
        false,
        Some("account-a"),
        false,
    );
    assert!(unrelated_failure.is_empty());

    let mut account_changed = no_owner;
    finalize_target(
        &mut account_changed,
        id,
        false,
        false,
        Some("account-b"),
        true,
    );
    assert_eq!(
        account_changed[0].owner_account_id.as_deref(),
        Some("account-a")
    );
}

#[test]
fn wrong_account_cannot_recheckpoint_or_rebind_an_ownerless_target() {
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let entry = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let targets = vec![entry];
    assert!(validate_target_account_binding(&targets, &[id.into()], Some("account-b")).is_err());
    assert!(validate_target_account_binding(&targets, &[id.into()], None).is_err());
    assert!(validate_target_account_binding(&targets, &[id.into()], Some("account-a")).is_ok());
    assert_eq!(targets[0].offset, Some(42));
    assert_eq!(targets[0].owner_account_id.as_deref(), Some("account-a"));
}

#[test]
fn dispatch_marker_is_durable_before_any_ipc_send() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let home = std::env::temp_dir().join(format!("codex-dispatch-marker-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CODEX_HOME", &home);
    let _guard = TestCodexHomeGuard { path: home };
    let first = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let other = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    write_manifest(&[
        PendingTarget {
            id: first.into(),
            offset: Some(42),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        },
        PendingTarget {
            id: other.into(),
            offset: Some(84),
            awaiting_owner: true,
            captured_restart: false,
            owner_account_id: Some("account-a".into()),
        },
    ])
    .unwrap();
    assert!(mark_dispatch_attempt_for_account(first, Some("account-b")).is_err());
    assert_eq!(load_manifest().unwrap().len(), 2);
    mark_dispatch_attempt_for_account(first, Some("account-a")).unwrap();
    // A fresh process reload sees only the other target, even if the first
    // process crashed immediately before or after its IPC write.
    let reloaded = load_manifest().unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, other);
    assert!(reloaded[0].awaiting_owner);
    assert!(mark_dispatch_attempt_for_account(first, Some("account-a")).is_err());
}

#[test]
fn new_restart_preserves_an_older_ownerless_checkpoint_and_account() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let home =
        std::env::temp_dir().join(format!("codex-ownerless-preserve-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CODEX_HOME", &home);
    let _guard = TestCodexHomeGuard { path: home };
    let old = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let new = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    write_manifest(&[PendingTarget {
        id: old.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    }])
    .unwrap();
    save_pending(&[new.into()]).unwrap();
    let pending = load_manifest().unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].id, old);
    assert_eq!(pending[0].offset, Some(42));
    assert_eq!(pending[0].owner_account_id.as_deref(), Some("account-a"));
    assert!(pending[1].captured_restart);
    assert!(!pending[1].awaiting_owner);
    assert_eq!(load_ownerless_pending().unwrap(), [old]);
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
        awaiting_owner: false,
        captured_restart: false,
        owner_account_id: None,
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
fn load_pending_filters_stale_targets_without_writing_the_manifest() {
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
    let queued_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e404";

    let now = chrono::Utc::now().timestamp();
    let old_time = now - 4 * 86400; // 4 days ago
    let fresh_time = now - 300; // 5 minutes ago

    let database = temp_dir.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);\
         INSERT INTO threads VALUES ('{stale_id}', 0, 'user', {old_time}, '');\
         INSERT INTO threads VALUES ('{completed_id}', 0, 'user', {fresh_time}, '');\
         INSERT INTO threads VALUES ('{active_id}', 0, 'user', {fresh_time}, '');\
         INSERT INTO threads VALUES ('{queued_id}', 0, 'user', {fresh_time}, '');"
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
    create_rollout(
        queued_id,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t2","error":null}}"#,
    );
    let queue = temp_dir.join("queue_1.sqlite");
    let queue_sql = format!(
        "CREATE TABLE queued_items (thread_id TEXT, payload TEXT); INSERT INTO queued_items VALUES ('{queued_id}', 'restart-paused');"
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(queue_sql)
        .status()
        .unwrap()
        .success());

    // Pre-populate manifest with all three targets
    let initial_targets = vec![
        PendingTarget {
            id: stale_id.to_string(),
            offset: Some(10),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: completed_id.to_string(),
            offset: Some(20),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: active_id.to_string(),
            offset: Some(30),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: queued_id.to_string(),
            offset: Some(40),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        },
    ];
    assert!(write_manifest(&initial_targets).is_ok());

    // Calling load_pending must prune stale and completed targets AND persist the cleaned list to disk
    let pending = load_pending().unwrap();
    assert_eq!(pending, vec![active_id.to_string()]);

    // The completed turn stays eligible for deferred queued follow-up
    // recovery, even though load_pending excludes ownerless entries.
    let mut eligible = load_manifest().unwrap();
    super::manifest_store::prune_ineligible_targets(&temp_dir, &mut eligible).unwrap();
    assert_eq!(
        eligible
            .iter()
            .map(|target| target.id.as_str())
            .collect::<Vec<_>>(),
        [active_id, queued_id]
    );

    // The detection read is deliberately non-mutating; the recovery owner
    // prunes stale entries under the operation lock.
    let reloaded = load_manifest().unwrap();
    assert_eq!(reloaded.len(), 4);

    std::fs::write(&queue, b"not a sqlite database").unwrap();
    assert!(load_pending().is_err());
    assert_eq!(load_manifest().unwrap().len(), 4);

    // If active_id completes and targets becomes empty, write_manifest removes the file completely
    assert!(write_manifest(&[]).is_ok());
    assert!(!temp_dir.join("desktop-recovery.json").exists());
    assert!(load_pending().unwrap().is_empty());
}
