use super::app_lifecycle::AppLifecycle;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_desktop_switch_service::DistributionDesktopSwitchService;
use super::distribution_journal::DistributionJournal;
use super::distribution_plan::DistributionPlan;
use super::distribution_recovery_preflight_service::DistributionRecoveryPreflightService;
use super::distribution_request::DistributionRequest;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_helper::{make_account, TestEnv};
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

fn add_quota_interrupted_thread(home: &Path) -> String {
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"t1\",\"error\":{\"codex_error_info\":\"usage_limit_exceeded\"}}}\n").unwrap();
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
            make_account(
                "old",
                None,
                "old@example.test",
                "plus",
                0.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.test",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
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

#[test]
fn prepare_rollback_failure_retains_distribution_journal_before_desktop_stop() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
        Err(error) => error,
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
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
        Err(error) => error,
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
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
fn failed_recovery_preflight_keeps_desktop_running_and_restores_prior_checkpoint() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("recovery_preflight_before_stop");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
                "plus",
                0.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.test",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
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
    std::fs::write(&rollout, b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"t1\",\"error\":{\"codex_error_info\":\"usage_limit_exceeded\"}}}\n").unwrap();
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
        Err(error) => error,
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
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
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
