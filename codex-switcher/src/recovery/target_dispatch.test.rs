use super::{
    ipc_call_error::IpcCallError,
    manifest_store::finalize_target,
    observer::Observer,
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
    recovery_target::RecoveryTarget,
    target_dispatch::{handle_owner_resolution, revalidate_after_owner},
};
use crate::switcher::ThreadRolloutState;
use std::{fs::OpenOptions, io::Write, path::Path, process::Command, time::Instant};

fn target(home: &Path, id: &str) -> RecoveryTarget {
    let rollout = home.join(format!("rollout-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}\n",
    )
    .unwrap();
    RecoveryTarget {
        id: id.into(),
        state: ThreadRolloutState::ActiveInProgress,
        writer_locked: false,
        observer: Observer::checkpoint(rollout).unwrap(),
        existing_queue: 0,
        mounted_by_recovery: false,
        owner_unavailable: false,
        account_mismatch: false,
        dispatched: false,
        completed: false,
        failure: None,
        deadline: Instant::now(),
        execution_deadline_set: false,
        expected_turn_id: None,
        proof_observed_at: None,
    }
}

#[test]
fn failed_navigation_before_dispatch_keeps_original_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-navigation-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    assert!(handle_owner_resolution(
        Err(IpcCallError::Other("LaunchServices unavailable".into())),
        &mut candidate,
    )
    .is_err());
    assert!(candidate.owner_unavailable);
    assert!(!candidate.dispatched);
    let mut manifest = vec![PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    }];
    finalize_target(
        &mut manifest,
        id,
        candidate.owner_unavailable,
        candidate.dispatched,
        Some("account-a"),
        false,
    );
    assert_eq!(manifest[0].offset, Some(42));
    assert!(manifest[0].awaiting_owner);
    assert_eq!(manifest[0].owner_account_id.as_deref(), Some("account-a"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_wait_recheck_does_not_dispatch_after_a_new_turn_starts() {
    let home = std::env::temp_dir().join(format!("codex-owner-turn-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let mut file = OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap();
    file.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n").unwrap();
    assert!(
        !revalidate_after_owner(&home, &mut candidate, 0, 0, RecoveryMode::DeferredCaptured)
            .unwrap()
    );
    assert!(candidate.observer.evidence.started);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_wait_recheck_rejects_a_new_queued_follow_up() {
    let home = std::env::temp_dir().join(format!("codex-owner-queue-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    let mut candidate = target(&home, id);
    let queue = home.join("queue_1.sqlite");
    let sql = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    assert!(
        revalidate_after_owner(&home, &mut candidate, 0, 0, RecoveryMode::DeferredCaptured)
            .is_err()
    );
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}
