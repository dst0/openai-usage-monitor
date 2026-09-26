use super::{
    manifest_store::finalize_target,
    observer::Observer,
    pending_target::PendingTarget,
    recovery_service::{mark_dispatch_failure, mark_pre_dispatch_channel_failure},
    recovery_target::{record_target_state_at, RecoveryTarget},
};
use crate::switcher::ThreadRolloutState;
use crate::{distribution::WindowProcessIdentity, recovery::RecoveryBanner};
use std::{io::Write, time::Instant};

#[test]
fn foreground_recovery_scans_large_checkpoint_in_bounded_steps_before_claiming_proof() {
    let home = std::env::temp_dir().join(format!("codex-foreground-scan-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let rollout = home.join("rollout.jsonl");
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let checkpoint = std::fs::metadata(&rollout).unwrap().len();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    let record = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
        "x".repeat(8 * 1024)
    );
    for _ in 0..(32 * 1024 * 1024_usize).div_ceil(record.len()) {
        writer.write_all(record.as_bytes()).unwrap();
    }
    writer.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n").unwrap();
    writer
        .write_all(b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n")
        .unwrap();
    writer.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"new-turn\",\"error\":{\"message\":\"failed\"}}}\n").unwrap();
    drop(writer);
    let mut target = RecoveryTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e79".into(),
        state: ThreadRolloutState::ActiveInProgress,
        writer_locked: false,
        observer: Observer::checkpoint_at(rollout.clone(), checkpoint).unwrap(),
        scan_complete: false,
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
    };
    record_target_state_at(&mut target, Instant::now()).unwrap();
    assert!(target.observer.offset - checkpoint <= 16 * 1024 * 1024);
    assert!(!target.completed);
    assert!(target.failure.is_none());
    for _ in 0..4 {
        record_target_state_at(&mut target, Instant::now()).unwrap();
    }
    assert!(target.failure.is_some());
    assert!(!target.completed);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cold_mount_without_a_visible_window_must_not_enter_ipc_dispatch() {
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let mut banner = RecoveryBanner::without_window(process.clone());
    let result = banner
        .ensure_visible_after_owner_with(false, || Ok(RecoveryBanner::without_window(process)));
    assert!(
        result.is_err(),
        "owner proof alone cannot replace a visible panel"
    );
    assert!(!banner.has_visible_panel());
}

#[test]
fn desktop_ipc_startup_failure_keeps_undispatched_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-ipc-start-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let rollout = home.join("rollout.jsonl");
    std::fs::write(
        &rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}\n",
    )
    .unwrap();
    let mut target = RecoveryTarget {
        id: id.into(),
        state: ThreadRolloutState::ActiveInProgress,
        writer_locked: false,
        observer: Observer::checkpoint(rollout).unwrap(),
        scan_complete: false,
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
    };
    mark_pre_dispatch_channel_failure(&mut target, "Desktop IPC unavailable");
    assert!(target.owner_unavailable);
    assert!(!target.dispatched);
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
        target.owner_unavailable,
        target.dispatched,
        Some("account-a"),
        false,
    );
    assert_eq!(manifest[0].offset, Some(42));
    assert!(manifest[0].awaiting_owner);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn sqlite_failure_before_dispatch_keeps_checkpoint_but_uncertain_send_does_not() {
    let home = std::env::temp_dir().join(format!("codex-queue-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let rollout = home.join("rollout.jsonl");
    std::fs::write(
        &rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}\n",
    )
    .unwrap();
    let mut target = RecoveryTarget {
        id: id.into(),
        state: ThreadRolloutState::ActiveInProgress,
        writer_locked: false,
        observer: Observer::checkpoint(rollout).unwrap(),
        scan_complete: false,
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
    };
    mark_dispatch_failure(&mut target, "queue SQLite unavailable");
    assert!(target.owner_unavailable);
    assert!(!target.dispatched);
    target.owner_unavailable = false;
    target.dispatched = true;
    mark_dispatch_failure(&mut target, "IPC reply lost");
    assert!(!target.owner_unavailable);
    std::fs::remove_dir_all(home).unwrap();
}
