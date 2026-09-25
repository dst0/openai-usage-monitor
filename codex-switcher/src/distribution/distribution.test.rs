use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_decision_service::DistributionDecisionService;
use super::distribution_journal::DistributionJournal;
use super::distribution_outcome::DistributionStatus;
use super::distribution_request::DistributionRequest;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_helper::{make_account, TestEnv};
use super::window_capture_mode::WindowCaptureMode;
use crate::models::{AccountsFile, Settings};
use crate::storage::{load_accounts, read_active_auth_json, save_accounts};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[test]
fn test_candidate_skip_reasons() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("skip_reasons");

    let accounts = vec![
        make_account(
            "acc_depleted",
            Some("Depleted Acc"),
            "depleted@example.com",
            "pro",
            0.0,
            Some(100.0),
            0,
            Some(3600),
            None,
        ),
        make_account(
            "acc_weekly_zero",
            Some("Weekly Zero"),
            "weekly@example.com",
            "team",
            80.0,
            Some(0.0),
            0,
            Some(3600),
            None,
        ),
        make_account(
            "acc_error",
            Some("Error Acc"),
            "error@example.com",
            "team",
            90.0,
            Some(50.0),
            1,
            Some(3600),
            Some("HTTP 429 rate limit exceeded"),
        ),
        make_account(
            "acc_healthy_team",
            Some("Healthy Team"),
            "team@example.com",
            "team",
            95.0,
            Some(80.0),
            2,
            Some(1800),
            None,
        ),
        make_account(
            "acc_healthy_pro",
            Some("Healthy Pro"),
            "pro@example.com",
            "pro",
            85.0,
            Some(70.0),
            0,
            Some(7200),
            None,
        ),
    ];

    env.populate(accounts.clone(), Some("acc_depleted"), Some("acc_depleted"));

    let decision_service = DistributionDecisionService::new();
    let accounts_file = load_accounts().unwrap();
    let req = DistributionRequest::auto("quota_depleted");

    let plan = decision_service.evaluate(
        &accounts_file,
        Some("acc_depleted"),
        Some("acc_depleted"),
        false,
        &req,
    );

    assert_eq!(
        plan.target_app_id.as_deref(),
        Some("team@example.com:acc_healthy_team")
    );
    assert_eq!(
        plan.target_cli_id.as_deref(),
        Some("pro@example.com:acc_healthy_pro")
    );
    assert!(plan.has_changes());

    let cand_depleted = plan
        .evaluated_candidates
        .iter()
        .find(|c| c.account_id == "depleted@example.com:acc_depleted")
        .unwrap();
    assert!(!cand_depleted.eligible);
    assert_eq!(cand_depleted.skip_reason.as_deref(), Some("depleted_5h"));

    let cand_weekly = plan
        .evaluated_candidates
        .iter()
        .find(|c| c.account_id == "weekly@example.com:acc_weekly_zero")
        .unwrap();
    assert!(!cand_weekly.eligible);
    assert_eq!(
        cand_weekly.skip_reason.as_deref(),
        Some("weekly_exhausted_no_credits")
    );

    let cand_err = plan
        .evaluated_candidates
        .iter()
        .find(|c| c.account_id == "error@example.com:acc_error")
        .unwrap();
    assert!(!cand_err.eligible);
    assert!(cand_err
        .skip_reason
        .as_deref()
        .unwrap_or("")
        .contains("active_error"));

    let cand_team = plan
        .evaluated_candidates
        .iter()
        .find(|c| c.account_id == "team@example.com:acc_healthy_team")
        .unwrap();
    assert!(cand_team.eligible);
    assert!(cand_team.skip_reason.is_none());
}

#[test]
fn test_coordinator_decision_preserves_strategy_and_business_only_filters() {
    let active = make_account(
        "active",
        None,
        "active@example.com",
        "plus",
        0.0,
        None,
        0,
        None,
        None,
    );
    let soon = make_account(
        "soon",
        None,
        "soon@example.com",
        "team",
        70.0,
        None,
        0,
        Some(60),
        None,
    );
    let high = make_account(
        "high",
        None,
        "high@example.com",
        "pro",
        100.0,
        None,
        0,
        Some(3600),
        None,
    );
    let service = DistributionDecisionService::new();
    let request = DistributionRequest::auto("quota_exhausted");

    let mut settings = Settings::default();
    settings.strategy = "reset-first".to_string();
    settings.auto_switch_business_only = true;
    let reset_first = AccountsFile {
        active_account_id: Some("active".to_string()),
        settings: settings.clone(),
        accounts: vec![active.clone(), soon.clone(), high.clone()],
    };
    let plan = service.evaluate(
        &reset_first,
        Some("active"),
        Some("active"),
        false,
        &request,
    );
    assert_eq!(plan.target_app_id.as_deref(), Some("soon"));
    assert_eq!(plan.target_cli_id.as_deref(), Some("soon"));
    assert!(plan
        .evaluated_candidates
        .iter()
        .any(|candidate| candidate.account_id == "high"
            && candidate.skip_reason.as_deref() == Some("business_only_filter")));

    settings.strategy = "highest-quota".to_string();
    settings.auto_switch_business_only = false;
    let highest_quota = AccountsFile {
        active_account_id: Some("active".to_string()),
        settings,
        accounts: vec![active, soon, high],
    };
    let plan = service.evaluate(
        &highest_quota,
        Some("active"),
        Some("active"),
        false,
        &request,
    );
    assert_eq!(plan.target_app_id.as_deref(), Some("high"));
}

#[test]
fn test_stale_snapshot_already_optimal() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("already_optimal");

    let accounts = vec![
        make_account(
            "acc_team",
            Some("Team"),
            "team@example.com",
            "team",
            90.0,
            Some(80.0),
            2,
            Some(1800),
            None,
        ),
        make_account(
            "acc_pro",
            Some("Pro"),
            "pro@example.com",
            "pro",
            80.0,
            Some(70.0),
            0,
            Some(3600),
            None,
        ),
    ];

    // Already on optimal distribution: Desktop on acc_team, CLI on acc_pro
    env.populate(accounts, Some("acc_pro"), Some("acc_team"));

    let mock = Arc::new(MockAppLifecycle::new(true));
    let coordinator = DistributionCoordinator::with_lifecycle(mock.clone());
    let req = DistributionRequest::auto("periodic_refresh");
    let outcome = coordinator
        .execute(req)
        .expect("Coordinator execute should succeed");

    assert_eq!(outcome.status, DistributionStatus::NoActionNeeded);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert!(!outcome.restarted_desktop);
    assert!(
        outcome.message.contains("No action needed") || outcome.message.contains("already_optimal")
    );
}

#[test]
fn test_duplicate_overlapping_automatic_operations() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("in_flight");

    let accounts = vec![
        make_account(
            "acc_old",
            Some("Old"),
            "old@example.com",
            "plus",
            0.0,
            None,
            0,
            None,
            None,
        ),
        make_account(
            "acc_new",
            Some("New"),
            "new@example.com",
            "team",
            100.0,
            None,
            1,
            None,
            None,
        ),
    ];
    env.populate(accounts, Some("acc_old"), Some("acc_old"));

    // Simulate an existing active operation in the journal held by current process
    let _journal = DistributionJournal::create(
        env.home(),
        "op_dist_in_flight_123",
        "auto",
        "quota_exhaustion",
        Some("acc_new"),
        Some("acc_new"),
    )
    .unwrap();

    let mock = Arc::new(MockAppLifecycle::new(false));
    let coordinator = DistributionCoordinator::with_lifecycle(mock);
    let req = DistributionRequest::auto("second_trigger");
    let outcome = coordinator
        .execute(req)
        .expect("Coordinator should handle in-flight safely");

    assert_eq!(outcome.status, DistributionStatus::DeferredInFlight);
    assert!(!outcome.restarted_desktop);

    // Verify auth.json was untouched
    let auth = read_active_auth_json().unwrap();
    assert_eq!(auth.tokens.unwrap().access_token, "tok_acc_old");
}

#[test]
fn test_transaction_failure_logs_sanitized_correlated_outcome() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("failed_outcome");
    env.populate(
        vec![
            make_account(
                "old",
                Some("Old"),
                "old@example.com",
                "plus",
                0.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "new",
                Some("New"),
                "new@example.com",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_stop_error("private failure /Users/example/private --unsafe-argument");
    let coordinator = DistributionCoordinator::with_lifecycle(mock);

    let error = coordinator
        .execute(DistributionRequest::auto("failure_test"))
        .unwrap_err();
    assert!(error.contains("private failure"));

    let log_content = env.log_content();
    let correlated = log_content
        .lines()
        .filter(|line| line.contains("reason=failure_test"))
        .collect::<Vec<_>>();
    let operation_id = correlated[0]
        .split_whitespace()
        .find(|field| field.starts_with("op_id="))
        .unwrap();
    assert!(correlated.iter().all(|line| line.contains(operation_id)));
    assert!(correlated.iter().any(|line| line.contains("phase=OUTCOME")
        && line.contains("status=failed code=transaction_failed")));
    assert!(!log_content.contains("/Users/example/private"));
    assert!(!log_content.contains("--unsafe-argument"));
}

#[test]
fn test_invalid_journal_and_cooldown_fail_closed_with_outcomes() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("invalid_coordination_state");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                0.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "new",
                None,
                "new@example.com",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let coordinator =
        DistributionCoordinator::with_lifecycle(Arc::new(MockAppLifecycle::new(false)));

    std::fs::write(env.home().join("distribution-journal.json"), b"invalid").unwrap();
    assert!(coordinator
        .execute(DistributionRequest::auto("invalid_journal_test"))
        .is_err());
    std::fs::remove_file(env.home().join("distribution-journal.json")).unwrap();

    std::fs::write(env.home().join("desktop-automation-cooldown"), b"invalid\n").unwrap();
    assert!(coordinator
        .execute(DistributionRequest::auto("invalid_cooldown_test"))
        .is_err());

    let log_content = env.log_content();
    assert!(log_content.contains("reason=invalid_journal_test"));
    assert!(log_content.contains("status=failed code=journal_invalid"));
    assert!(log_content.contains("reason=invalid_cooldown_test"));
    assert!(log_content.contains("status=failed code=cooldown_state_invalid"));
    let auth = read_active_auth_json().unwrap();
    assert_eq!(auth.tokens.unwrap().access_token, "tok_old");
}

#[test]
fn test_trigger_reason_correlation_and_operation_id_continuity() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("correlation");

    let accounts = vec![
        make_account(
            "acc_old",
            Some("Old"),
            "old@example.com",
            "plus",
            0.0,
            None,
            0,
            None,
            None,
        ),
        make_account(
            "acc_new",
            Some("New"),
            "new@example.com",
            "team",
            100.0,
            None,
            1,
            None,
            None,
        ),
    ];
    env.populate(accounts, Some("acc_old"), Some("acc_old"));

    let mock = Arc::new(MockAppLifecycle::new(false));
    let coordinator = DistributionCoordinator::with_lifecycle(mock);
    let auto_req = DistributionRequest::auto("quota_exhausted");
    let auto_outcome = coordinator.execute(auto_req).unwrap();

    assert_eq!(auto_outcome.trigger, "auto");
    assert_eq!(auto_outcome.reason, "quota_exhausted");
    assert!(auto_outcome.operation_id.starts_with("op_dist_"));

    let log_content = env.log_content();
    let op_id = &auto_outcome.operation_id;
    let mut phases = Vec::new();
    for line in log_content.lines() {
        if line.contains("[AUDIT]") && line.contains("reason=quota_exhausted") {
            assert!(line.contains(&format!("op_id={op_id}")));
            assert!(line.contains("trigger=auto"));
            phases.push(line.to_string());
        }
    }
    for phase in ["REQUEST", "DECISION", "LOCK", "AUTH_COMMIT_CLI", "OUTCOME"] {
        assert!(
            phases
                .iter()
                .any(|line| line.contains(&format!("phase={phase}"))),
            "missing correlated phase {phase}: {phases:?}"
        );
    }
}

#[test]
fn test_logging_redaction_and_privacy() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("redaction");

    let secret_email = "sensitive.executive@confidential-corp.com";
    let accounts = vec![make_account(
        "acc_secret",
        Some("Executive"),
        secret_email,
        "business",
        100.0,
        None,
        5,
        None,
        None,
    )];
    env.populate(accounts, Some("acc_secret"), Some("acc_secret"));

    let mock = Arc::new(MockAppLifecycle::new(false));
    let coordinator = DistributionCoordinator::with_lifecycle(mock);
    let req = DistributionRequest::user("test_audit_privacy");
    let _ = coordinator.execute(req);
    DistributionAuditLogger::new(env.home().join("log").join("switcher.log")).log_action(
        "op_test",
        "TEST",
        "auto",
        "privacy_test",
        "failure /Users/example/private --secret-argument",
    );

    let log_content = env.log_content();
    assert!(
        !log_content.contains(secret_email),
        "Raw email address must never appear in audit logs"
    );
    assert!(
        !log_content.contains("tok_acc_secret"),
        "Auth token must never appear in audit logs"
    );
    assert!(
        !log_content.contains("rt_acc_secret"),
        "Refresh token must never appear in audit logs"
    );
    assert!(
        !log_content.contains("Executive"),
        "Account names must never appear in audit logs"
    );
    assert!(
        !log_content.contains("acc_secret"),
        "Raw account IDs must never appear in audit logs"
    );
    assert!(
        !log_content.contains("/Users/example/private"),
        "Paths must never appear in audit logs"
    );
    assert!(
        !log_content.contains("--secret-argument"),
        "Argv payloads must never appear in audit logs"
    );

    // Check file permissions
    let log_path = env.home().join("log").join("switcher.log");
    if let Ok(meta) = std::fs::metadata(&log_path) {
        let perms = meta.permissions().mode() & 0o777;
        assert_eq!(perms, 0o600, "Log file must have POSIX 0600 permissions");
    }
    let log_dir = env.home().join("log");
    if let Ok(meta) = std::fs::metadata(&log_dir) {
        let perms = meta.permissions().mode() & 0o777;
        assert_eq!(
            perms, 0o700,
            "Log directory must have POSIX 0700 permissions"
        );
    }
}

#[test]
fn test_partial_account_switch_terminal_and_cooldown() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("partial_terminal");

    let accounts = vec![
        make_account(
            "acc_old",
            Some("Old"),
            "old@example.com",
            "plus",
            0.0,
            None,
            0,
            None,
            None,
        ),
        make_account(
            "acc_target",
            Some("Target"),
            "target@example.com",
            "team",
            90.0,
            None,
            1,
            None,
            None,
        ),
    ];
    env.populate(accounts, Some("acc_old"), Some("acc_old"));

    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_recovery_error("Simulated desktop recovery verification timeout");

    let coordinator = DistributionCoordinator::with_lifecycle(mock.clone());
    let req = DistributionRequest::auto("quota_exhausted");
    let outcome = coordinator.execute(req).unwrap();

    assert_eq!(outcome.status, DistributionStatus::PartialSuccess);
    assert!(outcome.recovery_error.is_some());
    assert!(outcome.is_terminal());
    assert!(!outcome.should_caller_retry());

    // Desktop app session was committed to target
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "target@example.com:acc_target");

    // Cooldown must be armed
    let remaining = crate::recovery::automation_cooldown_remaining().unwrap();
    assert!(
        remaining.is_some(),
        "Cooldown must be armed after partial switch"
    );

    // A second automatic request during cooldown must be deferred
    let second_req = DistributionRequest::auto("quota_exhausted");
    let second_outcome = coordinator.execute(second_req).unwrap();
    assert_eq!(second_outcome.status, DistributionStatus::DeferredCooldown);
}

#[test]
fn test_at_most_one_desktop_restart() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("at_most_one_restart");

    let accounts = vec![
        make_account(
            "acc_old",
            Some("Old"),
            "old@example.com",
            "plus",
            0.0,
            None,
            0,
            None,
            None,
        ),
        make_account(
            "acc_team",
            Some("Team"),
            "team@example.com",
            "team",
            100.0,
            None,
            2,
            None,
            None,
        ),
        make_account(
            "acc_pro",
            Some("Pro"),
            "pro@example.com",
            "pro",
            90.0,
            None,
            0,
            None,
            None,
        ),
    ];
    env.populate(accounts, Some("acc_old"), Some("acc_old"));

    let mock = Arc::new(MockAppLifecycle::new(true));
    let coordinator = DistributionCoordinator::with_lifecycle(mock.clone());

    // Both need switch: App to acc_team, CLI to acc_pro
    let req = DistributionRequest::auto("dual_distribution_exhaustion");
    let outcome = coordinator.execute(req).unwrap();

    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(
        outcome.target_app_id.as_deref(),
        Some("team@example.com:acc_team")
    );
    assert_eq!(
        outcome.target_cli_id.as_deref(),
        Some("pro@example.com:acc_pro")
    );

    // Exactly one stop and one launch occurred
    assert_eq!(
        mock.stop_calls.load(Ordering::SeqCst),
        1,
        "Desktop must be stopped at most once"
    );
    assert_eq!(
        mock.launch_calls.load(Ordering::SeqCst),
        1,
        "Desktop must be launched at most once"
    );
    assert_eq!(
        mock.capture_calls.load(Ordering::SeqCst),
        1,
        "Window capture must happen before the single restart"
    );
    assert_eq!(
        mock.restore_calls.load(Ordering::SeqCst),
        1,
        "Window restore must run after the single relaunch"
    );

    // Verify desktop app session is on team@example.com:acc_team
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "team@example.com:acc_team");

    // Verify CLI active auth is on acc_pro
    let auth = read_active_auth_json().unwrap();
    assert_eq!(auth.tokens.unwrap().access_token, "tok_acc_pro");

    let log_content = env.log_content();
    let correlated = log_content
        .lines()
        .filter(|line| line.contains("reason=dual_distribution_exhaustion"))
        .collect::<Vec<_>>();
    for phase in [
        "REQUEST",
        "DECISION",
        "SHUTDOWN",
        "RELAUNCH",
        "RECOVERY_START",
        "RECOVERY_VERIFIED",
        "OUTCOME",
    ] {
        assert!(
            correlated
                .iter()
                .any(|line| line.contains(&format!("phase={phase}"))
                    && line.contains(&format!("op_id={}", outcome.operation_id))),
            "missing correlated phase {phase}: {correlated:?}"
        );
    }
}

#[test]
fn test_stale_journal_cleanup_and_recovery() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = TestEnv::new("stale_journal");

    let accounts = vec![make_account(
        "acc_team",
        Some("Team"),
        "team@example.com",
        "team",
        90.0,
        None,
        2,
        None,
        None,
    )];
    env.populate(accounts, Some("acc_team"), Some("acc_team"));

    // Simulate an abandoned journal with a dead PID (PID 9999999 is dead)
    let journal_data = r#"{
        "operation_id": "op_dist_stale_999",
        "pid": 9999999,
        "trigger": "auto",
        "reason": "crashed_run",
        "target_app_id": null,
        "target_cli_id": null,
        "phase": "stopping_desktop",
        "started_at": "2026-09-17T00:00:00Z",
        "updated_at": "2026-09-17T00:00:00Z"
    }"#;
    std::fs::write(env.home().join("distribution-journal.json"), journal_data).unwrap();

    let mock = Arc::new(MockAppLifecycle::new(false));
    let coordinator = DistributionCoordinator::with_lifecycle(mock);

    let req = DistributionRequest::user("manual_check");
    let outcome = coordinator.execute(req).unwrap();

    assert_eq!(outcome.status, DistributionStatus::NoActionNeeded);
    // Verify stale journal was cleaned up
    assert!(
        !env.home().join("distribution-journal.json").exists(),
        "Stale journal must be deleted"
    );
}

#[test]
fn test_window_restore_failure_is_a_distribution_recovery_failure() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("window_restore_failure");
    env.populate(
        vec![
            make_account(
                "acc_old",
                Some("Old"),
                "old@example.com",
                "plus",
                0.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "acc_target",
                Some("Target"),
                "target@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("acc_old"),
        Some("acc_old"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_restore_error("post-restore bounds mismatch");
    let coordinator = DistributionCoordinator::with_lifecycle(mock.clone());

    let outcome = coordinator
        .execute(DistributionRequest::auto("restore_verification"))
        .expect("account distribution should return a terminal outcome");

    assert_eq!(outcome.status, DistributionStatus::PartialSuccess);
    assert_eq!(mock.capture_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 1);
    assert!(outcome
        .recovery_error
        .as_deref()
        .unwrap_or_default()
        .contains("bounds mismatch"));
}

#[test]
fn windowless_desktop_still_switches_and_recovers_without_geometry_restore() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("windowless_switch");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
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
                "next@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_capture_mode(WindowCaptureMode::Absent);
    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.rebind_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *mock.require_window_on_stability.lock().unwrap(),
        Some(false)
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("next@example.com:next")
    );
    assert!(env.log_content().contains("phase=WINDOW_ABSENT"));
}

#[test]
fn window_access_failure_prevents_auth_change_and_restart() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("window_access_failure");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
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
                "next@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_capture_error("WINDOW_ACCESS_FAILED");
    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));
    assert!(result.is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(env.log_content().contains("phase=WINDOW_CAPTURE_FAILED"));
}

#[test]
fn disabled_window_preservation_bypasses_ax_and_switches() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("window_access_disabled_preservation");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
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
                "next@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mut accounts = load_accounts().unwrap();
    accounts.settings.preserve_window_bounds_on_restart = false;
    save_accounts(&accounts).unwrap();

    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_capture_error("WINDOW_ACCESS_FAILED");
    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();

    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(mock.capture_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *mock.require_window_on_stability.lock().unwrap(),
        Some(false)
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("next@example.com:next")
    );
    assert!(env.log_content().contains("phase=WINDOW_CAPTURE_SKIPPED"));
}

#[test]
fn banner_rebind_failure_does_not_block_desktop_recovery() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("banner_rebind_failure");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
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
                "next@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mut accounts = load_accounts().unwrap();
    accounts.settings.preserve_window_bounds_on_restart = false;
    save_accounts(&accounts).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_rebind_error("panel not visible");

    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();

    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(mock.rebind_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 1);
    assert!(env
        .log_content()
        .contains("phase=RECOVERY_BANNER_REBIND_FAILED"));
}

#[test]
fn disabled_preservation_process_inspection_failure_blocks_auth_change() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("windowless_process_mismatch");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
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
                "next@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mut accounts = load_accounts().unwrap();
    accounts.settings.preserve_window_bounds_on_restart = false;
    save_accounts(&accounts).unwrap();

    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_process_inspection_error("PROCESS_IDENTITY_REJECTED");
    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(env.log_content().contains("phase=WINDOW_CAPTURE_FAILED"));
}

#[test]
fn failed_shutdown_clears_recovery_state_before_a_retry() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("shutdown_failure_cleanup");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
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
                "next@example.com",
                "team",
                100.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_capture_mode(WindowCaptureMode::Absent);
    mock.set_stop_error("stop refused");
    let coordinator = DistributionCoordinator::with_lifecycle(mock.clone());
    assert!(coordinator
        .execute(DistributionRequest::auto("quota_exhausted"))
        .is_err());
    assert_eq!(mock.abort_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(!env.home().join("desktop-recovery.json").exists());
    *mock.stop_error.lock().unwrap() = None;
    let outcome = coordinator
        .execute(DistributionRequest::user("retry").with_preferred_app(Some("next".into())))
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
}
