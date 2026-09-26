use super::{build_status, coordinate_automatic_distribution_with, CliAuthFileIdentityService};
use crate::distribution::distribution_executor::DistributionExecutor;
use crate::distribution::distribution_outcome::{DistributionOutcome, DistributionStatus};
use crate::distribution::distribution_request::DistributionRequest;
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::models::{AccountsFile, Settings};
use crate::storage::{read_active_auth_json, write_active_auth_json};
use std::sync::Mutex;

struct DaemonRecordingExecutor {
    requests: Mutex<Vec<DistributionRequest>>,
    status: DistributionStatus,
}

impl DaemonRecordingExecutor {
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

impl DistributionExecutor for DaemonRecordingExecutor {
    fn execute(&self, request: DistributionRequest) -> Result<DistributionOutcome, String> {
        self.requests.lock().unwrap().push(request.clone());
        if self.status == DistributionStatus::DeferredCooldown {
            return Ok(DistributionOutcome::deferred_cooldown(
                "op_daemon",
                request.trigger.as_str(),
                &request.reason,
                "cooldown",
            ));
        }
        Ok(DistributionOutcome::no_action(
            "op_daemon",
            request.trigger.as_str(),
            &request.reason,
            None,
            None,
            "no action",
        ))
    }
}

fn depleted_accounts(enabled: bool) -> AccountsFile {
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
                0.0,
                None,
                0,
                None,
                None,
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
fn daemon_entrypoint_calls_executor_once_with_target_free_auto_request() {
    let executor = DaemonRecordingExecutor::new(DistributionStatus::NoActionNeeded);

    coordinate_automatic_distribution_with(&executor, &depleted_accounts(true), false).unwrap();

    let requests = executor.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].trigger.as_str(), "auto");
    assert_eq!(requests[0].reason, "quota_exhausted");
    assert!(requests[0].preferred_app_id.is_none());
    assert!(requests[0].preferred_cli_id.is_none());
}

#[test]
fn daemon_entrypoint_respects_disabled_and_weekly_suppression() {
    let executor = DaemonRecordingExecutor::new(DistributionStatus::NoActionNeeded);

    coordinate_automatic_distribution_with(&executor, &depleted_accounts(false), false).unwrap();
    coordinate_automatic_distribution_with(&executor, &depleted_accounts(true), true).unwrap();

    assert!(executor.requests().is_empty());
}

#[test]
fn daemon_entrypoint_returns_coordinator_cooldown_without_retry() {
    let executor = DaemonRecordingExecutor::new(DistributionStatus::DeferredCooldown);

    let outcome =
        coordinate_automatic_distribution_with(&executor, &depleted_accounts(true), false)
            .unwrap()
            .unwrap();

    assert_eq!(outcome.status, DistributionStatus::DeferredCooldown);
    assert_eq!(executor.requests().len(), 1);
}

#[test]
fn status_does_not_attribute_cli_quota_without_verified_auth_file() {
    let accounts = depleted_accounts(true);
    let active = &accounts.accounts[0];
    let status = build_status(&accounts, Some(active), &active.id, None);

    assert!(status.active_account_id.is_none());
    assert!(status.accounts.iter().all(|account| !account.is_active));
}

#[test]
fn cli_status_file_identity_requires_matching_live_auth_tokens() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("cli_status_identity");
    let account = make_account(
        "active",
        None,
        "active@example.com",
        "plus",
        45.0,
        None,
        0,
        None,
        None,
    );
    env.populate(vec![account.clone()], Some("active"), None);
    let binding = CliAuthFileIdentityService::verified_id(&account);
    assert!(binding.is_some());
    let status = build_status(
        &depleted_accounts(true),
        Some(&account),
        &account.id,
        binding,
    );
    assert_eq!(status.active_account_id.as_deref(), Some("active"));

    let mut auth = read_active_auth_json().unwrap();
    auth.tokens.as_mut().unwrap().access_token = "different-test-token".into();
    write_active_auth_json(&auth).unwrap();
    assert!(CliAuthFileIdentityService::verified_id(&account).is_none());
}
