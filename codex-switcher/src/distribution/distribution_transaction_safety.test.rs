use super::desktop_app_session::DesktopAppSession;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_journal::DistributionJournal;
use super::distribution_outcome::DistributionStatus;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_service::DistributionTransactionService;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_account_spec::TestAccountSpec;
use super::test_helper::TestEnv;
use super::{AppLifecycle, AppStopError, WindowCaptureMode, WindowProcessIdentity};
use crate::storage::{load_accounts, read_active_auth_json, write_active_auth_json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct HookedLifecycle {
    inner: MockAppLifecycle,
    home: PathBuf,
    rotate_during_recovery: bool,
    break_registry_on_second_probe: bool,
    break_journal_after_stop: bool,
    break_checkpoint_after_stop: bool,
    rotate_auth_after_stop: bool,
    rotate_identifiable_auth_after_stop: bool,
    break_registry_handoff_after_stop: bool,
    activate_writer_after_probe: Option<usize>,
    mutate_auth_after_probe: Option<usize>,
    mutate_auth_after_inspection: Option<usize>,
    fail_after_stop: bool,
    probes: AtomicUsize,
}

impl HookedLifecycle {
    fn new(home: PathBuf, running: bool) -> Self {
        Self {
            inner: MockAppLifecycle::new(running),
            home,
            rotate_during_recovery: false,
            break_registry_on_second_probe: false,
            break_journal_after_stop: false,
            break_checkpoint_after_stop: false,
            rotate_auth_after_stop: false,
            rotate_identifiable_auth_after_stop: false,
            break_registry_handoff_after_stop: false,
            activate_writer_after_probe: None,
            mutate_auth_after_probe: None,
            mutate_auth_after_inspection: None,
            fail_after_stop: false,
            probes: AtomicUsize::new(0),
        }
    }
}

fn write_external_auth(
    home: &std::path::Path,
    auth: &crate::models::AuthJson,
) -> Result<(), String> {
    // The official Desktop writer does not take the Monitor's private flock.
    // Re-entering the Monitor writer from a callback inside that flock would
    // deadlock this test instead of simulating an external auth change.
    let encoded = serde_json::to_vec_pretty(auth).map_err(|error| error.to_string())?;
    let temp = home.join(format!(
        "auth.external.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    drop(file);
    std::fs::rename(&temp, home.join("auth.json")).map_err(|error| error.to_string())?;
    std::fs::File::open(home)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

impl AppLifecycle for HookedLifecycle {
    fn is_app_running(&self) -> Result<bool, String> {
        let probe = self.probes.fetch_add(1, Ordering::SeqCst);
        if self.break_registry_on_second_probe && probe == 1 {
            let registry = self.home.join("accounts.json");
            std::fs::rename(&registry, self.home.join("accounts.before-test.json")).unwrap();
            std::fs::create_dir(&registry).unwrap();
        }
        let observed = self.inner.is_app_running()?;
        if self.activate_writer_after_probe == Some(probe) {
            self.inner.running.store(true, Ordering::SeqCst);
        }
        if self.mutate_auth_after_probe == Some(probe) {
            let mut auth = read_active_auth_json()?;
            auth.tokens
                .as_mut()
                .ok_or("missing synthetic test tokens")?
                .refresh_token = Some("synthetic-post-write-auth-change".into());
            write_external_auth(&self.home, &auth)?;
        }
        Ok(observed)
    }

    fn preflight_shutdown_windows(&self) -> Result<(), String> {
        self.inner.preflight_shutdown_windows()
    }

    fn stop_app(&self) -> Result<(), AppStopError> {
        self.inner.stop_app()?;
        if self.rotate_auth_after_stop {
            let mut auth = read_active_auth_json().unwrap();
            auth.tokens.as_mut().unwrap().refresh_token =
                Some("synthetic-post-stop-refresh".into());
            write_active_auth_json(&auth).unwrap();
        }
        if self.rotate_identifiable_auth_after_stop {
            let mut auth = read_active_auth_json().unwrap();
            auth.tokens.as_mut().unwrap().refresh_token =
                Some("synthetic-post-stop-identifiable-refresh".into());
            write_active_auth_json(&auth).unwrap();
        }
        if self.break_registry_handoff_after_stop {
            // Registry staging uses unpredictable names. Replace only this
            // isolated test home's registry path to fail the fresh locked read.
            let registry = self.home.join("accounts.json");
            std::fs::rename(&registry, self.home.join("accounts.before-test.json")).unwrap();
            std::fs::create_dir(&registry).unwrap();
        }
        if self.break_checkpoint_after_stop {
            let path = self.home.join("desktop-recovery.json");
            std::fs::write(&path, b"{invalid checkpoint").unwrap();
        }
        if self.break_journal_after_stop {
            let journal = self.home.join("distribution-journal.json");
            std::fs::rename(&journal, self.home.join("journal.before-test.json")).unwrap();
            std::fs::create_dir(&journal).unwrap();
        }
        if self.fail_after_stop {
            return Err(AppStopError::after("synthetic post-signal stop failure"));
        }
        Ok(())
    }

    fn launch_app(&self) -> Result<Vec<u32>, String> {
        self.inner.launch_app()
    }

    fn inspect_process(&self, pid: u32) -> Result<WindowProcessIdentity, String> {
        let identity = self.inner.inspect_process(pid)?;
        let count = self.inner.process_inspection_calls.load(Ordering::SeqCst);
        if self.mutate_auth_after_inspection == Some(count) {
            let mut auth = read_active_auth_json()?;
            auth.tokens
                .as_mut()
                .ok_or("missing synthetic test tokens")?
                .refresh_token = Some("synthetic-post-commit-auth-change".into());
            write_external_auth(&self.home, &auth)?;
        }
        Ok(identity)
    }

    fn capture_window_bounds(
        &self,
        operation_id: &str,
        targets: &[String],
        reason: &str,
        preserve_window_bounds: bool,
    ) -> Result<WindowCaptureMode, String> {
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
        self.inner.recover_threads(targets)?;
        if self.rotate_during_recovery {
            let mut auth = read_active_auth_json()?;
            let tokens = auth.tokens.as_mut().ok_or("Target auth has no tokens")?;
            let claims = URL_SAFE_NO_PAD.encode(r#"{"email":"next@example.test"}"#);
            tokens.access_token = format!("header.{claims}.signature");
            tokens.refresh_token = Some("synthetic-rotated-refresh".into());
            write_active_auth_json(&auth)?;
        }
        Ok(())
    }

    fn verify_desktop_stable(&self, pids: &[u32], require_window: bool) -> Result<(), String> {
        self.inner.verify_desktop_stable(pids, require_window)
    }

    fn notify_distribution_complete(&self) {
        self.inner.notify_distribution_complete();
    }
}

#[test]
fn stale_marker_and_registry_cannot_stop_a_different_live_desktop_account() {
    let env = TestEnv::new("stale_marker_vs_live_auth");
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
    let saved = load_accounts().unwrap();
    let next = saved
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap();
    let mut live = read_active_auth_json().unwrap();
    live.tokens = Some(next.tokens.clone());
    write_active_auth_json(&live).unwrap();
    let lifecycle = Arc::new(MockAppLifecycle::new(true));

    let result = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err());
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(lifecycle.launch_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert!(!env.home().join("distribution-journal.json").exists());
    assert_eq!(read_active_auth_json().unwrap().tokens, live.tokens);
}

#[test]
fn concurrent_recovery_lock_preserves_old_in_flight_distribution_journal() {
    let env = TestEnv::new("concurrent_recovery_preserves_journal");
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
    let mut journal = DistributionJournal::create(
        env.home(),
        "op_long_running",
        "auto",
        "recovering",
        Some("next"),
        Some("next"),
    )
    .unwrap();
    journal.started_at = (chrono::Utc::now() - chrono::Duration::minutes(6)).to_rfc3339();
    journal.save(env.home()).unwrap();
    let before = std::fs::read(DistributionJournal::journal_path(env.home())).unwrap();
    let active_before = read_active_auth_json().unwrap().tokens;
    let recovery_lease = crate::recovery::operation_lock().unwrap();
    let lifecycle = Arc::new(MockAppLifecycle::new(true));

    let _ = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::user("second_tick"));

    assert_eq!(
        std::fs::read(DistributionJournal::journal_path(env.home())).unwrap(),
        before,
        "a second distribution must not clear a live recovery's journal"
    );
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(read_active_auth_json().unwrap().tokens, active_before);
    drop(recovery_lease);
}

#[test]
fn offline_marker_save_failure_rolls_back_shared_auth_and_registry() {
    let env = TestEnv::new("offline_marker_save_failure");
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
        None,
    );
    let before_auth = read_active_auth_json().unwrap().tokens;
    let before_registry = load_accounts().unwrap();
    let marker = env.home().join("desktop-app-session.json");
    std::fs::create_dir(&marker).unwrap();
    let lifecycle = Arc::new(MockAppLifecycle::new(false));

    let result = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err());
    assert_eq!(read_active_auth_json().unwrap().tokens, before_auth);
    assert_eq!(
        load_accounts().unwrap().active_account_id,
        before_registry.active_account_id
    );
    assert!(marker.is_dir());
    assert!(!env.home().join("distribution-journal.json").exists());
    assert_eq!(lifecycle.launch_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn late_desktop_writer_blocks_offline_auth_replacement() {
    let env = TestEnv::new("late_writer_offline_switch");
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
        None,
    );
    let before_auth = read_active_auth_json().unwrap().tokens;
    let accounts = load_accounts().unwrap();
    let current = accounts.active_account_id.clone().unwrap();
    let target = accounts
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap()
        .id
        .clone();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), false);
    lifecycle.activate_writer_after_probe = Some(1);
    let lifecycle = Arc::new(lifecycle);
    let service = DistributionTransactionService::with_lifecycle(
        DistributionAuditLogger::default(),
        lifecycle.clone(),
    );
    let plan = DistributionPlan {
        current_app_id: None,
        current_cli_id: Some(current),
        target_app_id: Some(target.clone()),
        target_cli_id: Some(target),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: false,
        decision_reason: "late_writer".into(),
        evaluated_candidates: Vec::new(),
    };

    let result = service.execute(
        "op_late_writer",
        &plan,
        &DistributionRequest::user("late_writer"),
        accounts,
    );

    assert!(result.is_err());
    assert!(lifecycle.inner.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap().tokens, before_auth);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.test:old")
    );
}

#[test]
fn writer_appearing_after_prewrite_probe_cannot_report_offline_success() {
    let env = TestEnv::new("writer_after_prewrite_probe");
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
        None,
    );
    let accounts = load_accounts().unwrap();
    let current = accounts.active_account_id.clone().unwrap();
    let target = accounts
        .accounts
        .iter()
        .find(|a| a.account_id == "next")
        .unwrap()
        .id
        .clone();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), false);
    lifecycle.activate_writer_after_probe = Some(2);
    let lifecycle = Arc::new(lifecycle);
    let service = DistributionTransactionService::with_lifecycle(
        DistributionAuditLogger::default(),
        lifecycle.clone(),
    );
    let plan = DistributionPlan {
        current_app_id: None,
        current_cli_id: Some(current),
        target_app_id: Some(target.clone()),
        target_cli_id: Some(target),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: false,
        decision_reason: "writer_after_probe".into(),
        evaluated_candidates: Vec::new(),
    };

    let result = service.execute(
        "op_writer_after_probe",
        &plan,
        &DistributionRequest::user("writer_after_probe"),
        accounts,
    );

    assert!(
        result.is_err(),
        "late writer must prevent a Success outcome"
    );
    assert!(lifecycle.inner.running.load(Ordering::SeqCst));
    assert!(env.home().join("distribution-journal.json").exists());
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.test:old")
    );
    assert!(!env.home().join("desktop-app-session.json").exists());
}

#[test]
fn auth_changing_during_postwrite_readback_cannot_report_offline_success() {
    let env = TestEnv::new("auth_changes_during_readback");
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
        None,
    );
    let accounts = load_accounts().unwrap();
    let current = accounts.active_account_id.clone().unwrap();
    let target = accounts
        .accounts
        .iter()
        .find(|a| a.account_id == "next")
        .unwrap()
        .id
        .clone();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), false);
    lifecycle.mutate_auth_after_probe = Some(3);
    let service = DistributionTransactionService::with_lifecycle(
        DistributionAuditLogger::default(),
        Arc::new(lifecycle),
    );
    let plan = DistributionPlan {
        current_app_id: None,
        current_cli_id: Some(current),
        target_app_id: Some(target.clone()),
        target_cli_id: Some(target),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: false,
        decision_reason: "auth_readback_changed".into(),
        evaluated_candidates: Vec::new(),
    };

    let result = service.execute(
        "op_auth_readback_changed",
        &plan,
        &DistributionRequest::user("auth_readback_changed"),
        accounts,
    );

    assert!(result.is_err());
    assert!(env.home().join("distribution-journal.json").exists());
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.test:old")
    );
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("synthetic-post-write-auth-change")
    );
}

#[test]
fn auth_changing_during_running_desktop_commit_is_not_verified() {
    let env = TestEnv::new("auth_changes_during_running_commit");
    env.populate(
        vec![TestAccountSpec {
            id: "next",
            email: "next@example.test",
            plan: "team",
            sprint_pct: 90.0,
            ..TestAccountSpec::default()
        }
        .build()],
        Some("next"),
        Some("next"),
    );
    let mut accounts = load_accounts().unwrap();
    let target = accounts.active_account_id.clone().unwrap();
    let process = WindowProcessIdentity::new(9999, "123:456789").unwrap();
    DesktopAppSession::bound(target.clone(), target.clone(), process)
        .save(&env.home().join("desktop-app-session.json"))
        .unwrap();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), true);
    lifecycle.mutate_auth_after_inspection = Some(2);

    let result = DistributionAccountCommitService::commit_latest_desktop_auth(
        &lifecycle,
        env.home(),
        &mut accounts,
        &target,
    );

    assert!(
        result.is_err(),
        "Desktop auth changed during final registry verification"
    );
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("synthetic-post-commit-auth-change")
    );
}

#[test]
fn post_signal_shutdown_error_retains_recovery_journal() {
    let env = TestEnv::new("post_signal_failure_journal");
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
    let retained_retry = serde_json::json!({"version": 1, "targets": [{
        "id": "01a098c2-0fae-74d2-a80c-45d89e910e79",
        "offset": 42,
        "awaiting_owner": true,
        "captured_restart": true,
        "owner_account_id": "old@example.test:old"
    }]});
    std::fs::write(
        env.home().join("desktop-recovery.json"),
        retained_retry.to_string(),
    )
    .unwrap();
    let before_auth = read_active_auth_json().unwrap().tokens;
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), true);
    lifecycle.fail_after_stop = true;
    let lifecycle = Arc::new(lifecycle);

    let result = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err());
    assert_eq!(lifecycle.inner.stop_calls.load(Ordering::SeqCst), 1);
    assert!(!lifecycle.inner.running.load(Ordering::SeqCst));
    assert!(env.home().join("distribution-journal.json").exists());
    assert!(env.home().join("desktop-recovery.json").exists());
    assert_eq!(read_active_auth_json().unwrap().tokens, before_auth);
}

#[test]
fn same_owner_refresh_before_shutdown_is_preserved_during_switch() {
    let env = TestEnv::new("same_owner_pre_stop_refresh");
    let mut old = TestAccountSpec {
        id: "old",
        email: "old@example.test",
        plan: "plus",
        ..TestAccountSpec::default()
    }
    .build();
    let claims = URL_SAFE_NO_PAD.encode(r#"{"email":"old@example.test"}"#);
    old.tokens.access_token = format!("header.{claims}.signature");
    env.populate(
        vec![
            old,
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
    let mut refreshed = read_active_auth_json().unwrap();
    refreshed.tokens.as_mut().unwrap().refresh_token = Some("pre-stop-rotation".into());
    write_active_auth_json(&refreshed).unwrap();
    let lifecycle = Arc::new(MockAppLifecycle::new(true));

    let result = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();

    assert_eq!(result.status, DistributionStatus::Success);
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 1);
    let registry = load_accounts().unwrap();
    let saved_old = registry
        .accounts
        .iter()
        .find(|account| account.account_id == "old")
        .unwrap();
    assert_eq!(
        saved_old.tokens.refresh_token.as_deref(),
        Some("pre-stop-rotation")
    );
}

#[test]
fn failed_registry_handoff_relaunches_with_identifiable_rotated_auth() {
    let env = TestEnv::new("failed_rotated_registry_handoff");
    let mut old = TestAccountSpec {
        id: "old",
        email: "old@example.test",
        plan: "plus",
        ..TestAccountSpec::default()
    }
    .build();
    let claims = URL_SAFE_NO_PAD.encode(r#"{"email":"old@example.test"}"#);
    old.tokens.access_token = format!("header.{claims}.signature");
    env.populate(
        vec![
            old,
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
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), true);
    lifecycle.rotate_identifiable_auth_after_stop = true;
    lifecycle.break_registry_handoff_after_stop = true;
    let lifecycle = Arc::new(lifecycle);

    let result = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err());
    assert_eq!(
        lifecycle.inner.stop_calls.load(Ordering::SeqCst),
        1,
        "{result:?}"
    );
    assert_eq!(
        lifecycle.inner.launch_calls.load(Ordering::SeqCst),
        1,
        "{result:?}"
    );
    assert!(lifecycle.inner.running.load(Ordering::SeqCst));
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("synthetic-post-stop-identifiable-refresh")
    );
    assert!(env.home().join("distribution-journal.json").exists());
}

#[test]
fn post_stop_checkpoint_error_does_not_launch_with_changed_auth() {
    let env = TestEnv::new("post_stop_checkpoint_auth_race");
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
    let before = load_accounts().unwrap();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), true);
    lifecycle.break_checkpoint_after_stop = true;
    lifecycle.rotate_auth_after_stop = true;
    let lifecycle = Arc::new(lifecycle);

    let result = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err());
    assert_eq!(lifecycle.inner.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(lifecycle.inner.launch_calls.load(Ordering::SeqCst), 0);
    assert!(!lifecycle.inner.running.load(Ordering::SeqCst));
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("synthetic-post-stop-refresh")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id,
        before.active_account_id
    );
    assert!(env.home().join("distribution-journal.json").exists());
}

#[test]
fn target_removed_before_shutdown_never_stops_desktop() {
    let env = TestEnv::new("target_removed_after_shutdown");
    env.populate(
        vec![TestAccountSpec {
            id: "old",
            email: "old@example.test",
            plan: "plus",
            ..TestAccountSpec::default()
        }
        .build()],
        Some("old"),
        Some("old"),
    );
    let accounts = load_accounts().unwrap();
    let active_id = accounts.active_account_id.clone().unwrap();
    let before = read_active_auth_json().unwrap();
    let lifecycle = Arc::new(MockAppLifecycle::new(true));
    let service = DistributionTransactionService::with_lifecycle(
        DistributionAuditLogger::default(),
        lifecycle.clone(),
    );
    let plan = DistributionPlan {
        current_app_id: Some(active_id.clone()),
        current_cli_id: Some(active_id),
        target_app_id: Some("removed@example.test:removed".into()),
        target_cli_id: Some("removed@example.test:removed".into()),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: true,
        decision_reason: "target_removed".into(),
        evaluated_candidates: Vec::new(),
    };

    assert!(service
        .execute(
            "op_removed",
            &plan,
            &DistributionRequest::user("target_removed"),
            accounts
        )
        .is_err());
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(lifecycle.launch_calls.load(Ordering::SeqCst), 0);
    assert!(lifecycle.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap().tokens, before.tokens);
}

#[test]
fn failed_target_launch_does_not_leave_target_auth_in_a_stopped_desktop() {
    let env = TestEnv::new("failed_target_launch_rollback");
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
    let before_registry = load_accounts().unwrap();
    let lifecycle = Arc::new(MockAppLifecycle::new(true));
    lifecycle.set_launch_error("synthetic launch failure");

    let _ = DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(read_active_auth_json().unwrap().tokens, before_auth.tokens);
    assert_eq!(
        load_accounts().unwrap().active_account_id,
        before_registry.active_account_id
    );
}

#[test]
fn desktop_refresh_during_recovery_is_preserved_in_registry() {
    let env = TestEnv::new("desktop_refresh_during_recovery");
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
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), true);
    lifecycle.rotate_during_recovery = true;
    let outcome = DistributionCoordinator::with_lifecycle(Arc::new(lifecycle))
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();

    assert_eq!(outcome.status, DistributionStatus::Success);
    let saved = load_accounts().unwrap();
    let target = saved
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap();
    assert_eq!(
        target.tokens.refresh_token.as_deref(),
        Some("synthetic-rotated-refresh")
    );
    assert_eq!(
        target.tokens,
        read_active_auth_json().unwrap().tokens.unwrap()
    );
}

#[test]
fn cli_registry_save_error_rolls_back_auth_and_is_reported() {
    let env = TestEnv::new("cli_registry_save_error");
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
        None,
    );
    let before = read_active_auth_json().unwrap();
    let accounts = load_accounts().unwrap();
    let active_id = accounts.active_account_id.clone().unwrap();
    let target_id = accounts
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap()
        .id
        .clone();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), false);
    lifecycle.break_registry_on_second_probe = true;
    let service = DistributionTransactionService::with_lifecycle(
        DistributionAuditLogger::default(),
        Arc::new(lifecycle),
    );
    let plan = DistributionPlan {
        current_app_id: None,
        current_cli_id: Some(active_id),
        target_app_id: Some(target_id.clone()),
        target_cli_id: Some(target_id),
        app_switch_needed: false,
        cli_switch_needed: true,
        restart_required: false,
        decision_reason: "cli_only".into(),
        evaluated_candidates: Vec::new(),
    };

    let result = service.execute(
        "op_cli_save",
        &plan,
        &DistributionRequest::user("cli_only"),
        accounts,
    );
    assert!(
        result.is_err(),
        "failed registry persistence must not report success"
    );
    assert_eq!(read_active_auth_json().unwrap().tokens, before.tokens);

    std::fs::remove_dir(env.home().join("accounts.json")).unwrap();
    std::fs::rename(
        env.home().join("accounts.before-test.json"),
        env.home().join("accounts.json"),
    )
    .unwrap();
    let saved = load_accounts().unwrap();
    assert_eq!(
        saved.active_account_id.as_deref(),
        Some("old@example.test:old")
    );
}

#[test]
fn journal_update_failure_after_stop_relaunches_previous_desktop() {
    let env = TestEnv::new("journal_failure_after_stop");
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
    let before = read_active_auth_json().unwrap();
    let mut lifecycle = HookedLifecycle::new(env.home().to_path_buf(), true);
    lifecycle.break_journal_after_stop = true;
    let lifecycle = Arc::new(lifecycle);

    assert!(DistributionCoordinator::with_lifecycle(lifecycle.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .is_err());
    assert_eq!(lifecycle.inner.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(lifecycle.inner.launch_calls.load(Ordering::SeqCst), 1);
    assert!(lifecycle.inner.running.load(Ordering::SeqCst));
    assert_eq!(read_active_auth_json().unwrap().tokens, before.tokens);
}
