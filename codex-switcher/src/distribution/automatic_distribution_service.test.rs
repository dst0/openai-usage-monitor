use super::automatic_distribution_service::AutomaticDistributionService;
use super::automatic_distribution_source::AutomaticDistributionSource;
use super::distribution_executor::DistributionExecutor;
use super::distribution_outcome::{DistributionOutcome, DistributionStatus};
use super::distribution_request::DistributionRequest;
use super::distribution_trigger::DistributionTrigger;
use super::test_helper::make_account;
use crate::models::{AccountsFile, Settings};
use std::sync::Mutex;

struct RecordingExecutor {
    requests: Mutex<Vec<DistributionRequest>>,
    status: DistributionStatus,
}

impl RecordingExecutor {
    fn new(status: DistributionStatus) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            status,
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    fn only_request(&self) -> DistributionRequest {
        self.requests.lock().unwrap()[0].clone()
    }
}

impl DistributionExecutor for RecordingExecutor {
    fn execute(&self, request: DistributionRequest) -> Result<DistributionOutcome, String> {
        self.requests.lock().unwrap().push(request.clone());
        let outcome = match self.status {
            DistributionStatus::DeferredCooldown => DistributionOutcome::deferred_cooldown(
                "op_test",
                request.trigger.as_str(),
                &request.reason,
                "cooldown",
            ),
            _ => DistributionOutcome::no_action(
                "op_test",
                request.trigger.as_str(),
                &request.reason,
                None,
                None,
                "test",
            ),
        };
        Ok(outcome)
    }
}

fn accounts(active_percent: f64) -> AccountsFile {
    let settings = Settings {
        auto_switch_enabled: true,
        ..Settings::default()
    };
    AccountsFile {
        active_account_id: Some("active".into()),
        settings,
        accounts: vec![
            make_account(
                "active",
                Some("Active"),
                "active@example.com",
                "plus",
                active_percent,
                Some(100.0),
                0,
                None,
                None,
            ),
            make_account(
                "candidate",
                Some("Candidate"),
                "candidate@example.com",
                "team",
                100.0,
                Some(100.0),
                0,
                None,
                None,
            ),
        ],
    }
}

#[test]
fn daemon_quota_decision_invokes_coordinator_once() {
    let executor = RecordingExecutor::new(DistributionStatus::NoActionNeeded);
    let service = AutomaticDistributionService::new(&executor);

    let outcome = service
        .execute(AutomaticDistributionSource::Daemon, &accounts(0.0), false)
        .unwrap();

    assert!(outcome.is_some());
    assert_eq!(executor.request_count(), 1);
    let request = executor.only_request();
    assert_eq!(request.trigger, DistributionTrigger::Auto);
    assert_eq!(request.reason, "quota_exhausted");
    assert!(request.preferred_app_id.is_none());
    assert!(request.preferred_cli_id.is_none());
}

#[test]
fn wrapper_rate_limit_decision_invokes_coordinator_once() {
    let mut snapshot = accounts(80.0);
    snapshot.accounts[0].last_error = Some("HTTP 429 rate limit reached".into());
    let executor = RecordingExecutor::new(DistributionStatus::NoActionNeeded);
    let service = AutomaticDistributionService::new(&executor);

    service
        .execute(
            AutomaticDistributionSource::WrapperPreflight,
            &snapshot,
            false,
        )
        .unwrap();

    assert_eq!(executor.request_count(), 1);
    let request = executor.only_request();
    assert_eq!(request.trigger, DistributionTrigger::Auto);
    assert_eq!(request.reason, "wrapper_preflight_rate_limit");
}

#[test]
fn disabled_and_weekly_reset_suppressed_paths_do_not_request_distribution() {
    let mut disabled = accounts(0.0);
    disabled.settings.auto_switch_enabled = false;
    let executor = RecordingExecutor::new(DistributionStatus::NoActionNeeded);
    let service = AutomaticDistributionService::new(&executor);

    assert!(service
        .execute(AutomaticDistributionSource::Daemon, &disabled, false)
        .unwrap()
        .is_none());
    assert!(service
        .execute(AutomaticDistributionSource::Daemon, &accounts(0.0), true)
        .unwrap()
        .is_none());
    assert_eq!(executor.request_count(), 0);
}

#[test]
fn business_priority_reason_is_stable_and_target_free() {
    let mut snapshot = accounts(80.0);
    snapshot.settings.auto_switch_business_priority = true;
    let executor = RecordingExecutor::new(DistributionStatus::NoActionNeeded);
    let service = AutomaticDistributionService::new(&executor);

    service
        .execute(AutomaticDistributionSource::Daemon, &snapshot, false)
        .unwrap();

    let request = executor.only_request();
    assert_eq!(request.reason, "business_priority_return");
    assert!(request.preferred_app_id.is_none());
    assert!(request.preferred_cli_id.is_none());
}

#[test]
fn coordinator_cooldown_outcome_is_returned_without_retry() {
    let executor = RecordingExecutor::new(DistributionStatus::DeferredCooldown);
    let service = AutomaticDistributionService::new(&executor);

    let outcome = service
        .execute(AutomaticDistributionSource::Daemon, &accounts(0.0), false)
        .unwrap()
        .unwrap();

    assert_eq!(outcome.status, DistributionStatus::DeferredCooldown);
    assert_eq!(executor.request_count(), 1);
}
