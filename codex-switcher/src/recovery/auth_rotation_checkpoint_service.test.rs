use super::auth_rotation_checkpoint_service::AuthRotationCheckpointService;
use super::{
    manifest_store::{
        finalize_target, load_manifest, mark_dispatch_attempt_for_account,
        validate_target_account_binding, write_manifest,
    },
    recovery_checkpoint::checkpoint_targets_with,
    recovery_mode::RecoveryMode,
    restart_checkpoint_service::save_pending,
    target_dispatch::{
        should_dispatch_with_auth_rotation, should_resume_queued_with_auth_rotation,
    },
};
use crate::{storage::test_codex_home::TestCodexHome, switcher::ThreadRolloutState};
use std::{fs::OpenOptions, io::Write, path::PathBuf, process::Command};

const AUTH_ERROR: &str = "Your access token could not be refreshed because you have since logged out or signed in to another account. Please sign in again.";

fn record(kind: &str, turn: &str, error: Option<&str>) -> String {
    let mut payload = serde_json::json!({"type": kind, "turn_id": turn});
    if let Some(error) = error {
        payload["error"] = serde_json::json!({"message": error});
    }
    format!(
        "{}\n",
        serde_json::json!({"type": "event_msg", "payload": payload})
    )
}

#[test]
fn exact_auth_rotation_terminal_is_the_only_auto_eligible_error() {
    let expected = record("task_complete", "turn-a", Some(AUTH_ERROR));
    assert!(AuthRotationCheckpointService::interval_matches(
        expected.as_bytes(),
        "turn-a"
    ));
    for error in [
        "Your access token could not be refreshed. Please sign in again.",
        "policy blocked",
        "retry 5/5 failed",
    ] {
        assert!(!AuthRotationCheckpointService::interval_matches(
            record("task_complete", "turn-a", Some(error)).as_bytes(),
            "turn-a"
        ));
    }
}

#[test]
fn stop_new_turn_or_wrong_turn_cannot_authorize_resume() {
    let terminal = record("task_complete", "turn-a", Some(AUTH_ERROR));
    for interval in [
        format!("{}{}", record("turn_aborted", "turn-a", None), terminal),
        format!("{}{}", record("task_started", "turn-b", None), terminal),
        record("task_complete", "turn-b", Some(AUTH_ERROR)),
        format!("{}{}", terminal, record("user_message", "turn-c", None)),
        format!(
            "{}{}",
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user"}}),
            format!("\n{terminal}")
        ),
    ] {
        assert!(!AuthRotationCheckpointService::interval_matches(
            interval.as_bytes(),
            "turn-a"
        ));
    }
}

#[test]
fn incomplete_or_malformed_interval_fails_closed() {
    let terminal = record("task_complete", "turn-a", Some(AUTH_ERROR));
    assert!(!AuthRotationCheckpointService::interval_matches(
        terminal.trim_end().as_bytes(),
        "turn-a"
    ));
    assert!(!AuthRotationCheckpointService::interval_matches(
        b"not-json\n",
        "turn-a"
    ));
    assert!(!AuthRotationCheckpointService::interval_matches(
        b"", "turn-a"
    ));
}

fn new_active_rollout(label: &str) -> (TestCodexHome, String, PathBuf) {
    let env = TestCodexHome::new(label);
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79".to_string();
    let sessions = env.path().join("sessions");
    std::fs::create_dir(&sessions).unwrap();
    let path = sessions.join(format!("rollout-{id}.jsonl"));
    std::fs::write(
        &path,
        format!(
            "{}{}",
            record("task_started", "turn-a", None),
            record("user_message", "turn-a", None)
        ),
    )
    .unwrap();
    (env, id, path)
}

fn append(path: &PathBuf, text: &str) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

fn capture_and_prepare(env: &TestCodexHome, id: &String) {
    let ids = std::slice::from_ref(id);
    let revisions = AuthRotationCheckpointService::queue_revisions(env.path(), ids).unwrap();
    save_pending(ids).unwrap();
    AuthRotationCheckpointService::prepare(env.path(), ids, "A", "B", &revisions).unwrap();
}

#[test]
fn captured_active_turn_auth_failure_keeps_target_account_bound_retry() {
    let (env, id, rollout) = new_active_rollout("auth-rotation-captured");
    capture_and_prepare(&env, &id);
    let pre = load_manifest().unwrap();
    let evidence = pre[0].auth_rotation.as_ref().unwrap();
    assert_eq!(evidence.source_account_id, "A");
    assert_eq!(evidence.target_account_id, "B");
    assert!(!evidence.confirmed_after_stop);
    append(
        &rollout,
        &record("task_complete", "turn-a", Some(AUTH_ERROR)),
    );
    AuthRotationCheckpointService::finalize_after_stop(env.path(), std::slice::from_ref(&id))
        .unwrap();
    let mut targets = load_manifest().unwrap();
    assert!(
        targets[0]
            .auth_rotation
            .as_ref()
            .unwrap()
            .confirmed_after_stop
    );
    assert!(
        validate_target_account_binding(&targets, std::slice::from_ref(&id), Some("A")).is_err()
    );
    assert!(
        validate_target_account_binding(&targets, std::slice::from_ref(&id), Some("B")).is_ok()
    );
    let mut explicit_view = targets.clone();
    assert!(checkpoint_targets_with(
        &mut explicit_view,
        std::slice::from_ref(&id),
        RecoveryMode::ExplicitTarget,
        Some("A"),
        |_| Some(rollout.metadata().unwrap().len()),
    )
    .is_ok());
    assert!(should_dispatch_with_auth_rotation(
        ThreadRolloutState::InterruptedByError,
        0,
        RecoveryMode::CapturedRestart,
        true,
    ));
    assert!(!should_dispatch_with_auth_rotation(
        ThreadRolloutState::InterruptedByError,
        0,
        RecoveryMode::CapturedRestart,
        false,
    ));
    finalize_target(&mut targets, &id, true, false, Some("B"), false);
    write_manifest(&targets).unwrap();
    assert_eq!(targets[0].owner_account_id.as_deref(), Some("B"));
    assert!(targets[0].awaiting_owner);
    assert!(
        mark_dispatch_attempt_for_account(&id, Some("A"), RecoveryMode::DeferredCaptured).is_err()
    );
    assert!(
        mark_dispatch_attempt_for_account(&id, Some("B"), RecoveryMode::DeferredCaptured).is_ok()
    );
}

#[test]
fn prior_auth_failure_and_unrelated_errors_remain_explicit_only() {
    let (env, id, rollout) = new_active_rollout("auth-rotation-prior-error");
    append(
        &rollout,
        &record("task_complete", "turn-a", Some(AUTH_ERROR)),
    );
    capture_and_prepare(&env, &id);
    assert!(load_manifest().unwrap()[0].auth_rotation.is_none());
    AuthRotationCheckpointService::finalize_after_stop(env.path(), std::slice::from_ref(&id))
        .unwrap();
    assert!(load_manifest().unwrap()[0].auth_rotation.is_none());
    assert!(!should_resume_queued_with_auth_rotation(
        ThreadRolloutState::InterruptedByError,
        RecoveryMode::DeferredCaptured,
        false,
    ));
}

#[test]
fn non_auth_terminal_after_pre_stop_checkpoint_does_not_gain_evidence() {
    let (env, id, rollout) = new_active_rollout("auth-rotation-generic-error");
    capture_and_prepare(&env, &id);
    append(
        &rollout,
        &record("task_complete", "turn-a", Some("policy blocked")),
    );
    AuthRotationCheckpointService::finalize_after_stop(env.path(), std::slice::from_ref(&id))
        .unwrap();
    assert!(load_manifest().unwrap()[0].auth_rotation.is_none());
}

#[test]
fn replacement_rollout_cannot_splice_an_auth_error_onto_an_old_checkpoint() {
    let (env, id, rollout) = new_active_rollout("auth-rotation-replaced-rollout");
    capture_and_prepare(&env, &id);
    let replacement = rollout.with_extension("replacement");
    let mut contents = std::fs::read(&rollout).unwrap();
    contents.extend(record("task_complete", "turn-a", Some(AUTH_ERROR)).as_bytes());
    std::fs::write(&replacement, contents).unwrap();
    std::fs::rename(&replacement, &rollout).unwrap();
    AuthRotationCheckpointService::finalize_after_stop(env.path(), std::slice::from_ref(&id))
        .unwrap();
    assert!(load_manifest().unwrap()[0].auth_rotation.is_none());
}

#[test]
fn malformed_auth_evidence_cannot_be_serialized_for_dispatch() {
    let (env, id, _) = new_active_rollout("auth-rotation-invalid-evidence");
    capture_and_prepare(&env, &id);
    let original = load_manifest().unwrap();
    let mut oversized = original.clone();
    oversized[0].auth_rotation.as_mut().unwrap().turn_id = "x".repeat(129);
    assert!(write_manifest(&oversized).is_err());
    let mut unbound = original.clone();
    unbound[0].auth_rotation.as_mut().unwrap().rollout_ino = 0;
    assert!(write_manifest(&unbound).is_err());
    let mut wrong_mode = original;
    wrong_mode[0].captured_restart = false;
    assert!(write_manifest(&wrong_mode).is_err());
}

#[test]
fn queue_input_between_first_snapshot_and_checkpoint_blocks_auth_exception() {
    let (env, id, _) = new_active_rollout("auth-rotation-queued-during-capture");
    let queue = env.path().join("queue_1.sqlite");
    let setup = format!(
        "CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); \
         INSERT INTO queued_thread_revisions VALUES ('{id}', 1);"
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(setup)
        .status()
        .unwrap()
        .success());
    let ids = std::slice::from_ref(&id);
    let revisions = AuthRotationCheckpointService::queue_revisions(env.path(), ids).unwrap();
    save_pending(ids).unwrap();
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(format!(
            "UPDATE queued_thread_revisions SET revision=2 WHERE thread_id='{id}';"
        ))
        .status()
        .unwrap()
        .success());
    AuthRotationCheckpointService::prepare(env.path(), ids, "A", "B", &revisions).unwrap();
    assert!(load_manifest().unwrap()[0].auth_rotation.is_none());
}
