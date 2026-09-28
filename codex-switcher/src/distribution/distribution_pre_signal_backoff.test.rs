use super::automatic_distribution_backoff::AutomaticDistributionBackoff;
use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_journal::DistributionJournal;
use super::distribution_outcome::DistributionStatus;
use super::distribution_request::DistributionRequest;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_account_spec::TestAccountSpec;
use super::test_helper::TestEnv;
use crate::storage::{load_accounts, read_active_auth_json, save_accounts};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// The 2026-09-28 LaunchAgent case: a personal account is active, a business
/// account has quota again, window preservation is off, and the shutdown
/// window guard cannot read Accessibility.
fn business_return_env(prefix: &str) -> (TestEnv, Arc<MockAppLifecycle>) {
    let env = TestEnv::new(prefix);
    env.populate(
        vec![
            TestAccountSpec {
                id: "personal",
                email: "personal@example.com",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "business",
                email: "business@example.com",
                plan: "team",
                sprint_pct: 100.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("personal"),
        Some("personal"),
    );
    let mut accounts = load_accounts().unwrap();
    accounts.settings.preserve_window_bounds_on_restart = false;
    save_accounts(&accounts).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_preflight_error("WINDOW_ACCESS_FAILED");
    (env, mock)
}

fn auto_request() -> DistributionRequest {
    DistributionRequest::auto("business_priority_return")
}

fn operation_lines(log: &str, phase: &str) -> Vec<String> {
    log.lines()
        .filter(|line| line.contains(&format!(" phase={phase} ")))
        .map(str::to_owned)
        .collect()
}

#[test]
fn shutdown_window_guard_failure_logs_its_own_phase() {
    let (env, mock) = business_return_env("window_guard_phase");
    let before_auth = read_active_auth_json().unwrap();

    let error = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(auto_request())
        .unwrap_err();

    assert_eq!(error, "WINDOW_ACCESS_FAILED");
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(read_active_auth_json().unwrap(), before_auth);
    assert!(!DistributionJournal::journal_path(env.home()).exists());
    let log = env.log_content();
    let guard = operation_lines(&log, "SHUTDOWN_WINDOW_GUARD_FAILED");
    assert_eq!(guard.len(), 1, "{log}");
    assert!(guard[0].contains("[ERROR]"), "{}", guard[0]);
    assert!(guard[0].contains("WINDOW_ACCESS_FAILED"), "{}", guard[0]);
    assert!(
        guard[0].contains("Desktop was not signalled"),
        "{}",
        guard[0]
    );
    let outcome = operation_lines(&log, "OUTCOME");
    assert_eq!(outcome.len(), 1, "{log}");
    assert!(
        outcome[0]
            .contains("code=transaction_failed pre_signal_phase=SHUTDOWN_WINDOW_GUARD_FAILED"),
        "{}",
        outcome[0]
    );
}

#[test]
fn repeated_identical_pre_signal_failure_backs_off_automatic_distribution() {
    let (env, mock) = business_return_env("pre_signal_backoff");
    let backoff = Arc::new(AutomaticDistributionBackoff::default());
    let coordinator =
        DistributionCoordinator::with_lifecycle(mock.clone()).with_automatic_backoff(backoff);

    assert!(coordinator.execute(auto_request()).is_err());
    assert!(operation_lines(&env.log_content(), "AUTO_BACKOFF_ARMED").is_empty());
    assert!(coordinator.execute(auto_request()).is_err());
    let attempts = mock.preflight_calls.load(Ordering::SeqCst);
    assert_eq!(attempts, 2);
    let armed = operation_lines(&env.log_content(), "AUTO_BACKOFF_ARMED");
    assert_eq!(armed.len(), 1);
    assert!(armed[0].contains("failures=2"), "{}", armed[0]);
    assert!(
        armed[0].contains("SHUTDOWN_WINDOW_GUARD_FAILED"),
        "{}",
        armed[0]
    );

    let deferred = coordinator.execute(auto_request()).unwrap();

    assert_eq!(deferred.status, DistributionStatus::DeferredCooldown);
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), attempts);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    let log = env.log_content();
    assert_eq!(operation_lines(&log, "AUTO_BACKOFF_ACTIVE").len(), 1);
    assert_eq!(operation_lines(&log, "LOCK").len(), 2, "{log}");
}

#[test]
fn manual_distribution_is_not_held_by_automatic_backoff() {
    let (env, mock) = business_return_env("manual_ignores_backoff");
    let backoff = Arc::new(AutomaticDistributionBackoff::default());
    let coordinator =
        DistributionCoordinator::with_lifecycle(mock.clone()).with_automatic_backoff(backoff);
    assert!(coordinator.execute(auto_request()).is_err());
    assert!(coordinator.execute(auto_request()).is_err());
    assert_eq!(
        coordinator.execute(auto_request()).unwrap().status,
        DistributionStatus::DeferredCooldown
    );

    let manual = coordinator.execute(DistributionRequest::user("business_priority_return"));

    assert_eq!(manual.unwrap_err(), "WINDOW_ACCESS_FAILED");
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 3);
    // A manual failure neither extends nor clears the automatic backoff.
    assert_eq!(
        operation_lines(&env.log_content(), "AUTO_BACKOFF_ARMED").len(),
        1
    );
    assert_eq!(
        coordinator.execute(auto_request()).unwrap().status,
        DistributionStatus::DeferredCooldown
    );
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 3);
}

#[test]
fn a_different_automatic_cause_is_not_held_by_the_backoff() {
    let (_env, mock) = business_return_env("different_cause");
    let backoff = Arc::new(AutomaticDistributionBackoff::default());
    let coordinator =
        DistributionCoordinator::with_lifecycle(mock.clone()).with_automatic_backoff(backoff);
    assert!(coordinator.execute(auto_request()).is_err());
    assert!(coordinator.execute(auto_request()).is_err());

    let other = coordinator.execute(DistributionRequest::auto("quota_exhausted"));

    assert_eq!(other.unwrap_err(), "WINDOW_ACCESS_FAILED");
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 3);
}

#[test]
fn a_changed_pre_signal_failure_restarts_the_count() {
    let (env, mock) = business_return_env("changed_failure");
    let backoff = Arc::new(AutomaticDistributionBackoff::default());
    let coordinator =
        DistributionCoordinator::with_lifecycle(mock.clone()).with_automatic_backoff(backoff);
    assert!(coordinator.execute(auto_request()).is_err());
    mock.set_preflight_error("Desktop process set changed before shutdown");
    assert!(coordinator.execute(auto_request()).is_err());
    assert!(operation_lines(&env.log_content(), "AUTO_BACKOFF_ARMED").is_empty());

    assert!(coordinator.execute(auto_request()).is_err());

    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        operation_lines(&env.log_content(), "AUTO_BACKOFF_ARMED").len(),
        1
    );
}

#[test]
fn a_failure_after_the_signal_ends_the_streak() {
    let (env, mock) = business_return_env("post_signal_resets_streak");
    let backoff = Arc::new(AutomaticDistributionBackoff::default());
    let coordinator =
        DistributionCoordinator::with_lifecycle(mock.clone()).with_automatic_backoff(backoff);
    assert!(coordinator.execute(auto_request()).is_err());
    *mock.preflight_error.lock().unwrap() = None;
    *mock.corrupt_manifest_after_stop.lock().unwrap() =
        Some(env.home().join("desktop-recovery.json"));

    assert!(coordinator.execute(auto_request()).is_err());

    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 1);
    assert!(!operation_lines(&env.log_content(), "OUTCOME")[1].contains("pre_signal_phase"));
    *mock.corrupt_manifest_after_stop.lock().unwrap() = None;
    std::fs::remove_file(env.home().join("desktop-automation-cooldown")).unwrap();
    mock.set_preflight_error("WINDOW_ACCESS_FAILED");
    assert_eq!(
        coordinator.execute(auto_request()).unwrap_err(),
        "WINDOW_ACCESS_FAILED"
    );
    assert!(operation_lines(&env.log_content(), "AUTO_BACKOFF_ARMED").is_empty());
}

#[test]
fn the_journal_gate_and_cooldown_are_checked_before_the_backoff() {
    let (env, mock) = business_return_env("gates_before_backoff");
    let backoff = Arc::new(AutomaticDistributionBackoff::default());
    let coordinator =
        DistributionCoordinator::with_lifecycle(mock.clone()).with_automatic_backoff(backoff);
    assert!(coordinator.execute(auto_request()).is_err());
    assert!(coordinator.execute(auto_request()).is_err());
    DistributionJournal::create(
        env.home(),
        "op_other_distribution",
        "user",
        "manual switch",
        None,
        None,
    )
    .unwrap();

    let in_flight = coordinator.execute(auto_request()).unwrap();

    assert_eq!(in_flight.status, DistributionStatus::DeferredInFlight);
    DistributionJournal::clear(env.home()).unwrap();
    crate::recovery::arm_automation_cooldown().unwrap();
    let cooling = coordinator.execute(auto_request()).unwrap();
    assert_eq!(cooling.status, DistributionStatus::DeferredCooldown);
    let log = env.log_content();
    assert_eq!(operation_lines(&log, "COOLDOWN_ACTIVE").len(), 1, "{log}");
    assert!(
        operation_lines(&log, "AUTO_BACKOFF_ACTIVE").is_empty(),
        "{log}"
    );
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 2);
}
