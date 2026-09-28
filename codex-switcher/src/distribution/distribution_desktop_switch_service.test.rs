use super::app_lifecycle::AppLifecycle;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_desktop_rollback_service::DistributionDesktopRollbackService;
use super::distribution_desktop_switch_service::DistributionDesktopSwitchService;
use super::distribution_journal::DistributionJournal;
use super::distribution_plan::DistributionPlan;
use super::distribution_recovery_preflight_service::DistributionRecoveryPreflightService;
use super::distribution_request::DistributionRequest;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_account_spec::TestAccountSpec;
use super::test_helper::TestEnv;
use crate::storage::{load_accounts, read_active_auth_json};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::Ordering, Mutex},
};

fn available_recovery_channel() -> Result<(), String> {
    Ok(())
}

fn write_recent_quota_rollout(path: &Path) {
    let event = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "type": "event_msg",
        "payload": {
            "type": "task_complete",
            "turn_id": "t1",
            "error": {"codex_error_info": "usage_limit_exceeded"}
        }
    });
    std::fs::write(path, format!("{event}\n")).unwrap();
}

fn add_quota_interrupted_thread(home: &Path) -> String {
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    write_recent_quota_rollout(&rollout);
    let state = home.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT); INSERT INTO threads VALUES ('{id}', 0, 'user', {}, '{}');",
        chrono::Utc::now().timestamp(), rollout.display(),
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&state)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    assert_eq!(crate::switcher::detect_in_progress_threads(), [id]);
    id.into()
}

struct CheckpointObservingLifecycle {
    inner: MockAppLifecycle,
    checkpoint_path: PathBuf,
    observe_at_preflight: bool,
    observed: Mutex<Option<Value>>,
}

impl CheckpointObservingLifecycle {
    fn new(path: PathBuf, observe_at_preflight: bool) -> Self {
        Self {
            inner: MockAppLifecycle::new(true),
            checkpoint_path: path,
            observe_at_preflight,
            observed: Mutex::new(None),
        }
    }

    fn capture_staged(&self) {
        let staged: Value =
            serde_json::from_slice(&std::fs::read(&self.checkpoint_path).unwrap()).unwrap();
        *self.observed.lock().unwrap() = Some(staged);
    }

    fn staged(&self) -> Value {
        self.observed
            .lock()
            .unwrap()
            .clone()
            .expect("staged checkpoint was captured")
    }
}

impl AppLifecycle for CheckpointObservingLifecycle {
    fn is_app_running(&self) -> Result<bool, String> {
        self.inner.is_app_running()
    }
    fn preflight_shutdown_windows(&self) -> Result<(), String> {
        let result = self.inner.preflight_shutdown_windows();
        if self.observe_at_preflight && self.inner.preflight_calls.load(Ordering::SeqCst) == 2 {
            self.capture_staged();
        }
        result
    }
    fn stop_app(&self) -> Result<(), super::app_stop_error::AppStopError> {
        if !self.observe_at_preflight {
            self.capture_staged();
        }
        self.inner.stop_app()
    }
    fn launch_app(&self) -> Result<Vec<u32>, String> {
        self.inner.launch_app()
    }
    fn inspect_process(
        &self,
        pid: u32,
    ) -> Result<super::window_restore_process_identity::ProcessIdentity, String> {
        self.inner.inspect_process(pid)
    }
    fn capture_window_bounds(
        &self,
        operation_id: &str,
        targets: &[String],
        reason: &str,
        preserve_window_bounds: bool,
    ) -> Result<super::window_capture_mode::WindowCaptureMode, String> {
        self.inner
            .capture_window_bounds(operation_id, targets, reason, preserve_window_bounds)
    }
    fn restore_window_bounds(
        &self,
        pid: u32,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        self.inner.restore_window_bounds(pid, operation_id, reason)
    }
    fn capture_window_tasks(&self) -> Result<(), String> {
        self.inner.capture_window_tasks()
    }
    fn captured_window_task_count(&self) -> Result<usize, String> {
        self.inner.captured_window_task_count()
    }
    fn restore_window_tasks(
        &self,
        bound: &super::DesktopAppSession,
        phase: super::WindowTaskRestorePhase<'_>,
    ) {
        self.inner.restore_window_tasks(bound, phase);
    }
    fn finish_window_tasks(&self) -> Result<(), String> {
        self.inner.finish_window_tasks()
    }
    fn rebind_banner(&self, pid: u32) -> Result<(), String> {
        self.inner.rebind_banner(pid)
    }
    fn abort_recovery(&self) {
        self.inner.abort_recovery();
    }
    fn recover_threads(&self, targets: &[String]) -> Result<(), String> {
        self.inner.recover_threads(targets)
    }
    fn verify_desktop_stable(&self, pids: &[u32], require_window: bool) -> Result<(), String> {
        self.inner.verify_desktop_stable(pids, require_window)
    }
    fn notify_distribution_complete(&self) {
        self.inner.notify_distribution_complete();
    }
}

fn unavailable_recovery_channel() -> Result<(), String> {
    Err("synthetic Desktop IPC preflight failure".into())
}

fn unavailable_channel_with_broken_checkpoint_path() -> Result<(), String> {
    let path = crate::storage::codex_home().join("desktop-recovery.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    Err("synthetic Desktop IPC preflight failure".into())
}

fn rollback_failure_fixture(
    label: &str,
) -> (
    TestEnv,
    crate::models::AccountsFile,
    DistributionPlan,
    DistributionJournal,
    crate::models::AuthJson,
    std::path::PathBuf,
) {
    let env = TestEnv::new(label);
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 90.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        Some("old"),
    );
    let accounts = load_accounts().unwrap();
    let target = accounts
        .accounts
        .iter()
        .find(|a| a.account_id == "next")
        .unwrap()
        .id
        .clone();
    let current = accounts.active_account_id.clone().unwrap();
    let plan = DistributionPlan {
        current_app_id: Some(current.clone()),
        current_cli_id: Some(current),
        target_app_id: Some(target.clone()),
        target_cli_id: Some(target.clone()),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: true,
        decision_reason: "synthetic quota interruption".into(),
        evaluated_candidates: Vec::new(),
    };
    let checkpoint_path = env.home().join("desktop-recovery.json");
    std::fs::write(
        &checkpoint_path,
        json!({"version":1,"targets":[{
            "id":"01a098c2-0fae-74d2-a80c-45d89e910e79","offset":42,
            "awaiting_owner":true,"captured_restart":true,"owner_account_id":"old-owner"
        }]})
        .to_string(),
    )
    .unwrap();
    let journal = DistributionJournal::create(
        env.home(),
        "op_rollback_failure",
        "user",
        "synthetic quota interruption",
        Some(&target),
        Some(&target),
    )
    .unwrap();
    let before_auth = read_active_auth_json().unwrap();
    (env, accounts, plan, journal, before_auth, checkpoint_path)
}

fn assert_previous_window_restore(
    lifecycle: &MockAppLifecycle,
    home: &Path,
    previous_id: &str,
    expected_events: &[&str],
) {
    assert_eq!(*lifecycle.task_events.lock().unwrap(), expected_events);
    let restored = lifecycle.task_restore_sessions.lock().unwrap();
    assert_eq!(restored.len(), 1);
    let bound = super::desktop_app_session::DesktopAppSession::load_checked(
        &home.join("desktop-app-session.json"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(restored[0], bound);
    assert_eq!(bound.account_id, previous_id);
    assert_eq!(bound.cli_account_id.as_deref(), Some(previous_id));
    assert_eq!(
        bound.process.unwrap(),
        lifecycle.inspect_process(9999).unwrap()
    );
}

#[test]
fn post_stop_checkpoint_failure_restores_previous_window_tasks_after_relaunch() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("post_stop_window_restore");
    let previous_id = plan.current_app_id.as_deref().unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.task_window_count.store(2, Ordering::SeqCst);
    *lifecycle.corrupt_manifest_after_stop.lock().unwrap() = Some(checkpoint_path);
    let logger = DistributionAuditLogger::default();
    let error = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        available_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_post_stop_window_restore",
    )
    .err()
    .expect("post-stop checkpoint failure must roll back");
    assert!(error.contains("previous Desktop relaunched"), "{error}");
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
    assert_previous_window_restore(
        &lifecycle,
        env.home(),
        previous_id,
        &["launch", "restore", "finish"],
    );
}

#[test]
fn post_auth_commit_rollback_restores_previous_window_tasks_after_relaunch() {
    let (env, accounts, plan, _, previous_auth, _) =
        rollback_failure_fixture("post_commit_window_restore");
    let previous_id = plan.current_app_id.as_deref().unwrap();
    let target_id = plan.target_app_id.as_deref().unwrap();
    let lifecycle = MockAppLifecycle::new(false);
    let target = DistributionAccountCommitService::find_account(&accounts, target_id).unwrap();
    let (_, committed) =
        DistributionAccountCommitService::apply_auth_tokens(&lifecycle, &target).unwrap();
    let error = DistributionDesktopRollbackService::new(&lifecycle).after_auth_commit(
        env.home(),
        &accounts,
        previous_id,
        &previous_auth,
        &committed,
        "synthetic post-commit failure".into(),
    );
    assert!(error.contains("Desktop switch rolled back"), "{error}");
    assert_eq!(read_active_auth_json().unwrap(), previous_auth);
    assert_previous_window_restore(
        &lifecycle,
        env.home(),
        previous_id,
        &["launch", "restore", "finish"],
    );
}

#[test]
fn target_launch_failure_restores_previous_windows_and_reports_partial_restore() {
    let (env, mut accounts, plan, mut journal, before_auth, _) =
        rollback_failure_fixture("target_launch_window_restore");
    let previous_id = plan.current_app_id.as_deref().unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.task_window_count.store(2, Ordering::SeqCst);
    lifecycle.set_launch_error_on_call(1, "synthetic target launch failure");
    *lifecycle.task_finish_error.lock().unwrap() = Some("WINDOW_TASKS_PARTIAL".into());
    let logger = DistributionAuditLogger::default();
    let error = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        available_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_target_launch_window_restore",
    )
    .err()
    .expect("target launch failure must roll back");
    assert!(error.contains("synthetic target launch failure"), "{error}");
    assert!(error.contains("WINDOW_TASKS_PARTIAL"), "{error}");
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
    assert_previous_window_restore(
        &lifecycle,
        env.home(),
        previous_id,
        &["launch", "launch", "restore", "finish"],
    );
}

#[test]
fn prepare_rollback_failure_retains_distribution_journal_before_desktop_stop() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("prepare_rollback_failure");
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.set_preflight_error_on_call(2, "synthetic second window preflight failure");
    lifecycle.block_checkpoint_at_preflight(2, checkpoint_path.clone());
    let logger = DistributionAuditLogger::default();
    let result = DistributionDesktopSwitchService::new(&lifecycle, &logger).run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_rollback_failure",
    );
    let error = match result {
        Err(error) => {
            assert_eq!(
                error.pre_signal_phase(),
                Some("SHUTDOWN_WINDOW_GUARD_FAILED")
            );
            error.into_message()
        }
        Ok(_) => panic!("prepare must fail"),
    };
    assert!(error.contains("could not be restored"), "{error}");
    assert!(DistributionJournal::journal_path(env.home()).exists());
    assert!(checkpoint_path.is_dir());
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn before_signal_stop_rollback_failure_retains_distribution_journal() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("stop_rollback_failure");
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.set_stop_error("synthetic stop rejected before signal");
    lifecycle.block_checkpoint_at_stop_error(checkpoint_path.clone());
    let logger = DistributionAuditLogger::default();
    let result = DistributionDesktopSwitchService::new(&lifecycle, &logger).run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_rollback_failure",
    );
    let error = match result {
        Err(error) => {
            assert_eq!(error.pre_signal_phase(), Some("SHUTDOWN_FAILED"));
            error.into_message()
        }
        Ok(_) => panic!("stop must fail"),
    };
    assert!(error.contains("could not be restored"), "{error}");
    assert!(DistributionJournal::journal_path(env.home()).exists());
    assert!(checkpoint_path.is_dir());
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 1);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn prepare_rejection_clears_journal_only_after_prior_checkpoint_is_restored() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("prepare_rollback_success");
    let before_checkpoint: Value =
        serde_json::from_slice(&std::fs::read(&checkpoint_path).unwrap()).unwrap();
    let interrupted_id = add_quota_interrupted_thread(env.home());
    let lifecycle = CheckpointObservingLifecycle::new(checkpoint_path.clone(), true);
    lifecycle
        .inner
        .set_preflight_error_on_call(2, "synthetic second window preflight failure");
    let logger = DistributionAuditLogger::default();
    let result = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        available_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_rollback_failure",
    );
    assert!(result.is_err());
    let staged = lifecycle.staged();
    assert_ne!(
        staged, before_checkpoint,
        "prepare must stage a new target before rollback"
    );
    assert!(staged["targets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|target| target["id"] == interrupted_id));
    assert!(!DistributionJournal::journal_path(env.home()).exists());
    let restored: Value = serde_json::from_slice(&std::fs::read(checkpoint_path).unwrap()).unwrap();
    assert_eq!(restored, before_checkpoint);
    assert_eq!(lifecycle.inner.stop_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.inner.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn before_signal_stop_rejection_clears_journal_after_checkpoint_restore() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("stop_rollback_success");
    let before_checkpoint: Value =
        serde_json::from_slice(&std::fs::read(&checkpoint_path).unwrap()).unwrap();
    let interrupted_id = add_quota_interrupted_thread(env.home());
    let lifecycle = CheckpointObservingLifecycle::new(checkpoint_path.clone(), false);
    lifecycle
        .inner
        .set_stop_error("synthetic stop rejected before signal");
    let logger = DistributionAuditLogger::default();
    let result = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        available_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_rollback_failure",
    );
    assert!(result.is_err());
    let staged = lifecycle.staged();
    assert_ne!(
        staged, before_checkpoint,
        "stop must see a new staged target before rollback"
    );
    assert!(staged["targets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|target| target["id"] == interrupted_id));
    assert!(!DistributionJournal::journal_path(env.home()).exists());
    let restored: Value = serde_json::from_slice(&std::fs::read(checkpoint_path).unwrap()).unwrap();
    assert_eq!(restored, before_checkpoint);
    assert_eq!(lifecycle.inner.stop_calls.load(Ordering::SeqCst), 1);
    assert!(lifecycle.inner.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn selected_window_without_recovery_targets_still_requires_ipc_preflight() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("window_only_ipc_preflight");
    assert!(crate::switcher::detect_in_progress_threads().is_empty());
    let old_checkpoint: Value =
        serde_json::from_slice(&std::fs::read(&checkpoint_path).unwrap()).unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.task_window_count.store(1, Ordering::SeqCst);
    let logger = DistributionAuditLogger::default();
    let error = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        unavailable_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_rollback_failure",
    )
    .err()
    .expect("IPC preflight must reject the selected-window restart");
    assert!(error.contains("preflight"), "{error}");
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(lifecycle.launch_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert!(!DistributionJournal::journal_path(env.home()).exists());
    let restored: Value = serde_json::from_slice(&std::fs::read(checkpoint_path).unwrap()).unwrap();
    assert_eq!(restored, old_checkpoint);
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn zero_windows_without_recovery_targets_skip_ipc_preflight() {
    let (env, mut accounts, plan, mut journal, before_auth, _checkpoint_path) =
        rollback_failure_fixture("zero_window_ipc_fast_path");
    assert!(crate::switcher::detect_in_progress_threads().is_empty());
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.set_stop_error("synthetic pre-signal stop refusal");
    let logger = DistributionAuditLogger::default();
    let error = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        unavailable_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_rollback_failure",
    )
    .err()
    .expect("synthetic stop refusal must be reported");
    assert!(error.contains("stop"), "{error}");
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn failed_recovery_preflight_keeps_desktop_running_and_restores_prior_checkpoint() {
    let env = TestEnv::new("recovery_preflight_before_stop");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 90.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        Some("old"),
    );
    let before_auth = read_active_auth_json().unwrap();
    let mut accounts = load_accounts().unwrap();
    let current = accounts.active_account_id.clone().unwrap();
    let target = accounts
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap()
        .id
        .clone();
    let plan = DistributionPlan {
        current_app_id: Some(current.clone()),
        current_cli_id: Some(current),
        target_app_id: Some(target.clone()),
        target_cli_id: Some(target.clone()),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: true,
        decision_reason: "synthetic quota interruption".into(),
        evaluated_candidates: Vec::new(),
    };

    let old_checkpoint = json!({"version": 1, "targets": [{
        "id": "01a098c2-0fae-74d2-a80c-45d89e910e79", "offset": 42,
        "awaiting_owner": true, "captured_restart": true,
        "owner_account_id": "old-owner"
    }]});
    let checkpoint_path = env.home().join("desktop-recovery.json");
    std::fs::write(&checkpoint_path, old_checkpoint.to_string()).unwrap();

    let interrupted_id = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    let sessions = env.home().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!(
        "rollout-2026-09-26T00-00-00-{interrupted_id}.jsonl"
    ));
    write_recent_quota_rollout(&rollout);
    let state = env.home().join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT); INSERT INTO threads VALUES ('{interrupted_id}', 0, 'user', {}, '{}');",
        chrono::Utc::now().timestamp(),
        rollout.display()
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&state)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        crate::switcher::detect_in_progress_threads(),
        [interrupted_id]
    );

    let mut journal = DistributionJournal::create(
        env.home(),
        "op_recovery_preflight",
        "user",
        "synthetic quota interruption",
        Some(&target),
        Some(&target),
    )
    .unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    lifecycle.set_stop_error("stop must not be called after preflight failure");
    let logger = DistributionAuditLogger::default();
    let result = DistributionDesktopSwitchService::with_preflight(
        &lifecycle,
        &logger,
        unavailable_recovery_channel,
    )
    .run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_recovery_preflight",
    );
    let error = match result {
        Err(error) => {
            assert_eq!(error.pre_signal_phase(), Some("RECOVERY_PREFLIGHT_FAILED"));
            error.into_message()
        }
        Ok(_) => panic!("failed recovery preflight must reject Desktop shutdown"),
    };

    assert!(error.contains("preflight"), "{error}");
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert!(!DistributionJournal::journal_path(env.home()).exists());
    let restored: Value = serde_json::from_slice(&std::fs::read(checkpoint_path).unwrap()).unwrap();
    assert_eq!(restored, old_checkpoint);
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
}

#[test]
fn failed_preflight_retains_distribution_journal_when_checkpoint_rollback_fails() {
    let env = TestEnv::new("recovery_preflight_rollback_failure");
    let checkpoint_path = env.home().join("desktop-recovery.json");
    std::fs::write(
        &checkpoint_path,
        json!({"version": 1, "targets": [{
            "id": "01a098c2-0fae-74d2-a80c-45d89e910e79", "offset": 42,
            "awaiting_owner": true, "captured_restart": true,
            "owner_account_id": "old-owner"
        }]})
        .to_string(),
    )
    .unwrap();
    let checkpoint = crate::recovery::RecoveryManifestSnapshot::capture().unwrap();
    DistributionJournal::create(
        env.home(),
        "op_failed_preflight_rollback",
        "user",
        "synthetic quota interruption",
        None,
        None,
    )
    .unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    let logger = DistributionAuditLogger::default();
    let error = DistributionRecoveryPreflightService::with_dispatch(
        &lifecycle,
        &logger,
        unavailable_channel_with_broken_checkpoint_path,
    )
    .run(
        env.home(),
        &checkpoint,
        &["01a098c2-0fae-74d2-a80c-45d89e910e80".into()],
        "op_failed_preflight_rollback",
        &DistributionRequest::user("synthetic quota interruption"),
    )
    .unwrap_err();

    assert!(
        error.contains("checkpoint could not be restored"),
        "{error}"
    );
    assert!(checkpoint_path.is_dir());
    assert!(DistributionJournal::journal_path(env.home()).exists());
    assert!(lifecycle.running.load(Ordering::SeqCst));
}

#[test]
fn unreadable_checkpoint_clears_the_unsignalled_journal() {
    let (env, mut accounts, plan, mut journal, before_auth, checkpoint_path) =
        rollback_failure_fixture("checkpoint_capture_failure");
    std::fs::remove_file(&checkpoint_path).unwrap();
    std::fs::create_dir(&checkpoint_path).unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    let logger = DistributionAuditLogger::default();
    let result = DistributionDesktopSwitchService::new(&lifecycle, &logger).run(
        env.home(),
        &mut journal,
        &plan,
        &DistributionRequest::user("synthetic quota interruption"),
        &mut accounts,
        "op_checkpoint_capture",
    );
    let Err(error) = result else {
        panic!("an unreadable checkpoint must stop the switch");
    };

    assert_eq!(error.pre_signal_phase(), Some("RECOVERY_CHECKPOINT_FAILED"));
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert!(!DistributionJournal::journal_path(env.home()).exists());
    assert!(checkpoint_path.is_dir());
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
    assert!(env
        .log_content()
        .contains("phase=RECOVERY_CHECKPOINT_FAILED trigger=user"));
}
