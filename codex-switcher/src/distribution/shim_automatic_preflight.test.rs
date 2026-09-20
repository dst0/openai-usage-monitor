use super::coordinate_wrapper_automatic_distribution_with;
use crate::distribution::distribution_executor::DistributionExecutor;
use crate::distribution::distribution_outcome::{DistributionOutcome, DistributionStatus};
use crate::distribution::distribution_request::DistributionRequest;
use crate::distribution::test_helper::make_account;
use crate::models::{AccountsFile, Settings};
use std::sync::Mutex;

struct WrapperRecordingExecutor {
    requests: Mutex<Vec<DistributionRequest>>,
    status: DistributionStatus,
}

impl WrapperRecordingExecutor {
    fn new(status: DistributionStatus) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            status,
        }
    }

    fn requests(&self) -> Vec<DistributionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl DistributionExecutor for WrapperRecordingExecutor {
    fn execute(&self, request: DistributionRequest) -> Result<DistributionOutcome, String> {
        self.requests.lock().unwrap().push(request.clone());
        if self.status == DistributionStatus::DeferredCooldown {
            return Ok(DistributionOutcome::deferred_cooldown(
                "op_wrapper",
                request.trigger.as_str(),
                &request.reason,
                "cooldown",
            ));
        }
        Ok(DistributionOutcome::no_action(
            "op_wrapper",
            request.trigger.as_str(),
            &request.reason,
            None,
            None,
            "no action",
        ))
    }
}

fn rate_limited_accounts(enabled: bool) -> AccountsFile {
    let mut settings = Settings::default();
    settings.auto_switch_enabled = enabled;
    AccountsFile {
        active_account_id: Some("active".to_string()),
        settings,
        accounts: vec![
            make_account(
                "active",
                None,
                "active@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                Some("HTTP 429 rate limit reached"),
            ),
            make_account(
                "reserve",
                None,
                "reserve@example.com",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            ),
        ],
    }
}

#[test]
fn wrapper_entrypoint_calls_executor_once_with_preflight_reason() {
    let executor = WrapperRecordingExecutor::new(DistributionStatus::NoActionNeeded);
    let args = vec!["exec".to_string(), "task".to_string()];

    coordinate_wrapper_automatic_distribution_with(&executor, &args, &rate_limited_accounts(true))
        .unwrap();

    let requests = executor.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].trigger.as_str(), "auto");
    assert_eq!(requests[0].reason, "wrapper_preflight_rate_limit");
    assert!(requests[0].preferred_app_id.is_none());
    assert!(requests[0].preferred_cli_id.is_none());
}

#[test]
fn wrapper_entrypoint_skips_metadata_and_disabled_automation() {
    let executor = WrapperRecordingExecutor::new(DistributionStatus::NoActionNeeded);

    coordinate_wrapper_automatic_distribution_with(
        &executor,
        &["--version".to_string()],
        &rate_limited_accounts(true),
    )
    .unwrap();
    coordinate_wrapper_automatic_distribution_with(
        &executor,
        &["exec".to_string()],
        &rate_limited_accounts(false),
    )
    .unwrap();

    assert!(executor.requests().is_empty());
}

#[test]
fn wrapper_entrypoint_returns_coordinator_cooldown_without_retry() {
    let executor = WrapperRecordingExecutor::new(DistributionStatus::DeferredCooldown);

    let outcome = coordinate_wrapper_automatic_distribution_with(
        &executor,
        &["exec".to_string()],
        &rate_limited_accounts(true),
    )
    .unwrap()
    .unwrap();

    assert_eq!(outcome.status, DistributionStatus::DeferredCooldown);
    assert_eq!(executor.requests().len(), 1);
}
