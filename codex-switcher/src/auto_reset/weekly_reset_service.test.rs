use super::WeeklyResetService;
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::reset_journal::ResetJournal;
use crate::auto_reset::weekly_reset_policy::{episode_key, unresolved_attempt};
use crate::auto_reset::{load_journal_at, unresolved_auto_reset_for, write_journal_at};
use crate::distribution::test_helper::TestEnv;
use crate::models::{AccountConfig, AuthJson, Settings};
use crate::quota::ResetCreditConsumeOutcome;
use crate::storage::{update_accounts_atomically, write_active_auth_json};
use std::path::PathBuf;
use std::sync::MutexGuard;

const ACCOUNT_ID: &str = "user@example.invalid:account-id";
const BLOCKED_TASK: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e4a1";
const PRIOR_KEY: &str = "00000000-0000-4000-8000-000000000001";

type Change = fn();
type EnvironmentFactory = fn() -> FakeWeeklyResetEnvironment;

/// Each change lands after the lock-time registry check, while tasks are being
/// detected. No request may be sent and no journal may claim that one was.
#[test]
fn change_during_preparation_never_strands_an_unsent_pending_attempt() {
    let scenarios: [(&str, Change, &str); 9] = [
        (
            "weekly pool restored",
            || edit_account(|a| a.last_weekly_percentage = Some(35.0)),
            "policy_changed",
        ),
        (
            "usage read failed",
            || edit_account(|a| a.last_error = Some("synthetic".into())),
            "policy_changed",
        ),
        (
            "credit already spent",
            || edit_account(|a| a.last_credits = Some(0)),
            "no_credit",
        ),
        (
            "reset window closing",
            || edit_account(|a| a.last_weekly_reset_after_seconds = Some(3_600)),
            "waiting_for_window",
        ),
        (
            "automatic reset disabled",
            || edit_registry(|r| r.settings.auto_reset_weekly_enabled = false),
            "policy_changed",
        ),
        (
            "reset threshold changed",
            || edit_registry(|r| r.settings.auto_reset_weekly_min_remaining_seconds = 7_200),
            "policy_changed",
        ),
        (
            "active account switched",
            switch_active_account,
            "policy_changed",
        ),
        (
            "active account removed",
            || edit_registry(|r| r.accounts.clear()),
            "policy_changed",
        ),
        ("live auth rotated", rotate_live_auth, "policy_changed"),
    ];
    for (label, change, expected_state) in scenarios {
        let home = Home::prepare();
        let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).during_detection(change);
        let report = run(&environment);
        let journal_written = home.journal_path().exists();
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        let other = WeeklyResetService::maybe_consume_weekly_reset(
            &settings(),
            &other_account(),
            &FakeWeeklyResetEnvironment::new(&[]),
        );
        drop(home);

        let report = report.unwrap_or_else(|error| panic!("{label}: {error}"));
        assert!(
            environment.requests().is_empty(),
            "{label}: a request was sent"
        );
        assert!(!journal_written, "{label}: an unsent attempt was persisted");
        assert!(
            report.status.state == expected_state && !report.suppress_auto_switch,
            "{label}: unexpected no-send report {:?}",
            (report.status.state, report.suppress_auto_switch)
        );
        assert_eq!(
            route_blocked,
            Ok(false),
            "{label}: manual reset stayed blocked"
        );
        let other = other.unwrap_or_else(|error| panic!("{label}: {error}"));
        assert!(
            other.status.state == "ready" && !other.suppress_auto_switch,
            "{label}: another account stayed behind the unsent attempt ({})",
            other.status.state
        );
    }
}

/// A persisted attempt may already have reached the service. A refusal on a
/// retry must neither send nor demote it, including when Desktop is closed.
#[test]
fn refused_retry_keeps_a_possibly_sent_attempt_unresolved() {
    let refusals: [(&str, EnvironmentFactory); 3] = [
        ("live auth rotated", || {
            FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).during_detection(rotate_live_auth)
        }),
        ("active account switched", || {
            FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).during_detection(switch_active_account)
        }),
        ("desktop closed", || {
            FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_desktop(Ok(false))
        }),
    ];
    for state in ["pending", "unknown"] {
        for (label, environment) in refusals {
            let home = Home::prepare();
            write_journal_at(&home.journal_path(), &prior_attempt(state)).unwrap();
            let before = std::fs::read(home.journal_path()).unwrap();
            let environment = environment();
            let report = run(&environment);
            let after = std::fs::read(home.journal_path()).unwrap();
            let route_blocked = unresolved_auto_reset_for(&snapshot());
            drop(home);

            let report = report.unwrap_or_else(|error| panic!("{state}/{label}: {error}"));
            assert!(
                environment.requests().is_empty(),
                "{state}/{label}: a request was sent"
            );
            assert_eq!(
                before, after,
                "{state}/{label}: the uncertain attempt was rewritten"
            );
            assert!(
                report.status.state == state && report.suppress_auto_switch,
                "{state}/{label}: rotation was allowed during an uncertain attempt"
            );
            assert_eq!(
                route_blocked,
                Ok(true),
                "{state}/{label}: manual reset was unblocked"
            );
        }
    }
}

#[test]
fn closed_desktop_defers_without_pending_and_the_retry_is_pending_when_sent() {
    let home = Home::prepare();
    let closed = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_desktop(Ok(false));
    let deferred = run(&closed).unwrap();
    let waiting = load_journal_at(&home.journal_path()).unwrap();
    let manual_open = unresolved_auto_reset_for(&snapshot());
    let open = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_outcome(
        ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into()),
    );
    let sent = run(&open).unwrap();
    let settled = load_journal_at(&home.journal_path()).unwrap();
    drop(home);

    assert!(
        closed.requests().is_empty(),
        "a request was sent without Desktop"
    );
    assert!(
        deferred.status.state == "waiting_for_desktop" && !deferred.suppress_auto_switch,
        "unexpected deferred report"
    );
    assert!(!unresolved_attempt(&waiting) && waiting.state == "waiting_for_desktop");
    assert_eq!(waiting.thread_id.as_deref(), Some(BLOCKED_TASK));
    assert_eq!(
        manual_open,
        Ok(false),
        "a deferred, unsent attempt blocked manual reset"
    );
    let key = waiting
        .idempotency_key
        .clone()
        .expect("deferred attempt keeps its key");
    assert_eq!(
        open.requests(),
        vec![("pending".to_string(), Some(key.clone()), key)],
        "the retried request left before its pending marker was durable"
    );
    assert!(settled.state == "not_consumed" && !sent.suppress_auto_switch);
}

#[test]
fn ready_attempt_is_pending_on_disk_when_the_request_leaves() {
    let outcomes = [
        (
            ResetCreditConsumeOutcome::Unknown("synthetic_transport".into()),
            "unknown",
            true,
        ),
        (
            ResetCreditConsumeOutcome::Unavailable("synthetic_unavailable".into()),
            "waiting_for_service",
            false,
        ),
        (
            ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into()),
            "not_consumed",
            false,
        ),
    ];
    for (outcome, expected_state, suppress) in outcomes {
        let home = Home::prepare();
        let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_outcome(outcome);
        let report = run(&environment).unwrap();
        let journal = load_journal_at(&home.journal_path()).unwrap();
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        drop(home);

        let key = journal
            .idempotency_key
            .clone()
            .expect("attempt keeps its key");
        assert_eq!(
            environment.requests(),
            vec![("pending".to_string(), Some(key.clone()), key)],
            "{expected_state}: request left without a durable pending marker"
        );
        assert_eq!(journal.state, expected_state);
        assert_eq!(report.suppress_auto_switch, suppress, "{expected_state}");
        assert_eq!(route_blocked, Ok(suppress), "{expected_state}: manual gate");
    }
}

#[test]
fn eligible_retry_reuses_the_persisted_key_without_demoting_it_first() {
    let home = Home::prepare();
    write_journal_at(&home.journal_path(), &prior_attempt("unknown")).unwrap();
    let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_outcome(
        ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into()),
    );
    let report = run(&environment).unwrap();
    let journal = load_journal_at(&home.journal_path()).unwrap();
    drop(home);

    assert_eq!(
        environment.requests(),
        vec![(
            "unknown".to_string(),
            Some(PRIOR_KEY.to_string()),
            PRIOR_KEY.to_string()
        )]
    );
    assert!(journal.state == "not_consumed" && !report.suppress_auto_switch);
}

#[test]
fn desktop_probe_failure_sends_and_persists_nothing() {
    let home = Home::prepare();
    let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK])
        .with_desktop(Err("synthetic process inspection failure".into()));
    let result = run(&environment);
    let journal_written = home.journal_path().exists();
    drop(home);

    assert!(
        result.is_err(),
        "an unknown Desktop state was treated as an answer"
    );
    assert!(environment.requests().is_empty() && !journal_written);
}

/// Hermetic `CODEX_HOME` holding one weekly-exhausted active account whose
/// registry, settings, and live auth agree with the daemon's snapshot.
struct Home {
    env: Option<TestEnv>,
    _guard: MutexGuard<'static, ()>,
}

impl Home {
    fn prepare() -> Self {
        let guard = crate::setup::TEST_CODEX_HOME_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let env = TestEnv::new("auto_reset_dispatch");
        env.populate(vec![snapshot()], Some(ACCOUNT_ID), None);
        edit_registry(|registry| {
            registry.settings.auto_reset_weekly_enabled = true;
            registry.settings.auto_reset_weekly_min_remaining_seconds =
                settings().auto_reset_weekly_min_remaining_seconds;
        });
        Self {
            env: Some(env),
            _guard: guard,
        }
    }

    fn journal_path(&self) -> PathBuf {
        self.env
            .as_ref()
            .unwrap()
            .home()
            .join("auto-reset-state.json")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        self.env.take();
        std::env::remove_var("CODEX_HOME");
    }
}

fn run(
    environment: &FakeWeeklyResetEnvironment,
) -> Result<crate::auto_reset::AutoResetReport, String> {
    WeeklyResetService::maybe_consume_weekly_reset(&settings(), &snapshot(), environment)
}

fn prior_attempt(state: &str) -> ResetJournal {
    ResetJournal {
        episode_key: Some(episode_key(&snapshot())),
        account_id: Some(snapshot().account_id),
        thread_id: Some(BLOCKED_TASK.into()),
        idempotency_key: Some(PRIOR_KEY.into()),
        state: state.into(),
        reason: Some("synthetic_prior_reason".into()),
        updated_at: Some("2026-01-01T00:00:00Z".into()),
        ..ResetJournal::default()
    }
}

fn edit_registry(change: impl FnOnce(&mut crate::models::AccountsFile)) {
    update_accounts_atomically(|registry| {
        change(registry);
        Ok(())
    })
    .unwrap();
}

fn edit_account(change: impl FnOnce(&mut AccountConfig)) {
    edit_registry(|registry| change(&mut registry.accounts[0]));
}

fn switch_active_account() {
    edit_registry(|registry| {
        registry.accounts.push(other_account());
        registry.active_account_id = Some(other_account().id);
    });
}

fn rotate_live_auth() {
    let mut tokens = snapshot().tokens;
    tokens.access_token = "rotated-not-a-real-token".into();
    write_active_auth_json(&AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(tokens),
        last_refresh: None,
        extra: Default::default(),
    })
    .unwrap();
}

fn settings() -> Settings {
    Settings {
        auto_reset_weekly_enabled: true,
        auto_reset_weekly_min_remaining_seconds: 86_400,
        ..Settings::default()
    }
}

fn snapshot() -> AccountConfig {
    let mut account = crate::distribution::test_helper::make_account(
        ACCOUNT_ID,
        None,
        "user@example.invalid",
        "team",
        0.0,
        Some(0.0),
        1,
        None,
        None,
    );
    account.account_id = "account-id".into();
    account.tokens.account_id = Some("account-id".into());
    account.last_weekly_reset_time = Some("2026-01-08T00:00:00Z".into());
    account.last_weekly_reset_after_seconds = Some(100_000);
    account
}

fn other_account() -> AccountConfig {
    let mut account = snapshot();
    account.id = "other@example.invalid:other-route".into();
    account.email = "other@example.invalid".into();
    account.account_id = "other-route".into();
    account.tokens.account_id = Some("other-route".into());
    account.tokens.access_token = "other-not-a-real-token".into();
    account.last_weekly_percentage = Some(80.0);
    account
}
