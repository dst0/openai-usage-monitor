use super::{
    coordinate_automatic_distribution_with, merge_quota_caches, persist_quota_caches_with_hook,
    refresh_quota_caches_with,
};
use crate::distribution::distribution_executor::DistributionExecutor;
use crate::distribution::distribution_outcome::{DistributionOutcome, DistributionStatus};
use crate::distribution::distribution_request::DistributionRequest;
use crate::distribution::test_helper::make_account;
use crate::models::{AccountsFile, Settings};
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
fn quota_merge_cannot_replay_stale_active_tokens_over_desktop_refresh() {
    let mut stale_poll = depleted_accounts(false);
    let mut latest_registry = depleted_accounts(false);
    stale_poll.accounts[0].tokens.access_token = "active-before-desktop-refresh".into();
    stale_poll.accounts[0].tokens.refresh_token = Some("old-refresh".into());
    stale_poll.accounts[0].last_primary_percentage = 17.0;
    stale_poll.accounts[1].tokens.access_token = "reserve-before-switch".into();
    stale_poll.accounts[1].tokens.refresh_token = Some("reserve-old-refresh".into());
    latest_registry.accounts[0].tokens.access_token = "active-after-desktop-refresh".into();
    latest_registry.accounts[0].tokens.refresh_token = Some("latest-refresh".into());
    latest_registry.accounts[1].tokens.access_token = "reserve-after-switch".into();
    latest_registry.accounts[1].tokens.refresh_token = Some("reserve-latest-refresh".into());

    merge_quota_caches(&stale_poll, &mut latest_registry);

    assert_eq!(latest_registry.accounts[0].last_primary_percentage, 17.0);
    assert_eq!(
        latest_registry.accounts[0].tokens.access_token,
        "active-after-desktop-refresh"
    );
    assert_eq!(
        latest_registry.accounts[0].tokens.refresh_token.as_deref(),
        Some("latest-refresh")
    );
    assert_eq!(
        latest_registry.accounts[1].tokens.access_token,
        "reserve-after-switch"
    );
    assert_eq!(
        latest_registry.accounts[1].tokens.refresh_token.as_deref(),
        Some("reserve-latest-refresh")
    );
}

#[test]
fn daemon_does_not_authorize_oauth_refresh_for_any_account() {
    let mut accounts = depleted_accounts(false);
    let mut observed = Vec::new();

    refresh_quota_caches_with(&mut accounts, |account, allow_refresh| {
        observed.push((account.id.clone(), allow_refresh));
        if allow_refresh {
            account.tokens.refresh_token = Some("rotated-by-monitor".into());
        }
    });

    assert_eq!(
        observed,
        vec![("active".into(), false), ("reserve".into(), false)]
    );
    assert_eq!(
        accounts.accounts[0].tokens.refresh_token.as_deref(),
        Some("rt_active")
    );
    assert_eq!(
        accounts.accounts[1].tokens.refresh_token.as_deref(),
        Some("rt_reserve")
    );
}

#[test]
fn quota_save_cannot_overwrite_relogin_between_read_and_commit() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let home =
        std::env::temp_dir().join(format!("daemon-relogin-interleave-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CODEX_HOME", &home);

    let mut stale_poll = depleted_accounts(false);
    stale_poll.accounts[0].last_primary_percentage = 12.0;
    crate::storage::save_accounts(&stale_poll).unwrap();
    let mut relogged = stale_poll.clone();
    relogged.accounts[0].tokens.access_token = "new-browser-access".into();
    relogged.accounts[0].tokens.refresh_token = Some("new-browser-refresh".into());
    let result =
        persist_quota_caches_with_hook(&stale_poll, || crate::storage::save_accounts(&relogged));
    let final_registry = crate::storage::load_accounts().unwrap();
    std::env::remove_var("CODEX_HOME");
    std::fs::remove_dir_all(&home).unwrap();

    result.unwrap();
    assert_eq!(
        final_registry.accounts[0].tokens,
        relogged.accounts[0].tokens
    );
    assert_eq!(final_registry.accounts[0].last_primary_percentage, 12.0);
}
