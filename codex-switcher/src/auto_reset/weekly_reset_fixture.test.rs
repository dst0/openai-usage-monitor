use super::WeeklyResetService;
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::reset_journal::ResetJournal;
use crate::auto_reset::weekly_reset_policy::episode_key;
use crate::auto_reset::AutoResetReport;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::models::{AccountConfig, AccountsFile, AuthJson, Settings};
use crate::storage::{update_accounts_atomically, write_active_auth_json};
use std::path::PathBuf;

pub(super) const ACCOUNT_ID: &str = "user@example.invalid:account-id";
pub(super) const BLOCKED_TASK: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e4a1";
pub(super) const OTHER_TASK: &str = "01a07d3c-3008-75c2-87a6-2c5c75f0e4a2";
pub(super) const PRIOR_KEY: &str = "00000000-0000-4000-8000-000000000001";

/// Hermetic `CODEX_HOME` holding one weekly-exhausted active account whose
/// registry, settings, and live auth agree with the daemon's snapshot.
///
/// `TestEnv` owns the only `TestCodexHome` guard for the test, which holds the
/// serial lock, sets and clears `CODEX_HOME`, and releases both on drop,
/// including while a failed assertion unwinds. Do not lock the serial mutex or
/// create another guard here: the mutex is not reentrant.
pub(super) struct Home {
    env: TestEnv,
}

impl Home {
    pub(super) fn prepare() -> Self {
        let env = TestEnv::new("auto_reset_dispatch");
        env.populate(vec![snapshot()], Some(ACCOUNT_ID), None);
        edit_registry(|registry| {
            registry.settings.auto_reset_weekly_enabled = true;
            registry.settings.auto_reset_weekly_min_remaining_seconds =
                settings().auto_reset_weekly_min_remaining_seconds;
        });
        Self { env }
    }

    pub(super) fn journal_path(&self) -> PathBuf {
        self.env.home().join("auto-reset-state.json")
    }

    pub(super) fn write_journal(&self, journal: &ResetJournal) -> Vec<u8> {
        crate::auto_reset::write_journal_at(&self.journal_path(), journal).unwrap();
        std::fs::read(self.journal_path()).unwrap()
    }

    pub(super) fn journal_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.journal_path()).ok()
    }
}

pub(super) fn run(environment: &FakeWeeklyResetEnvironment) -> Result<AutoResetReport, String> {
    WeeklyResetService::maybe_consume_weekly_reset(&settings(), &snapshot(), environment)
}

pub(super) fn prior_attempt(state: &str) -> ResetJournal {
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

pub(super) fn edit_registry(change: impl FnOnce(&mut AccountsFile)) {
    update_accounts_atomically(|registry| {
        change(registry);
        Ok(())
    })
    .unwrap();
}

pub(super) fn edit_account(change: impl FnOnce(&mut AccountConfig)) {
    edit_registry(|registry| change(&mut registry.accounts[0]));
}

/// A different, independently authenticated account becomes active.
pub(super) fn switch_active_account() {
    edit_registry(|registry| {
        registry.accounts.push(other_account());
        registry.active_account_id = Some(other_account().id);
    });
}

pub(super) fn write_live_tokens(change: impl FnOnce(&mut crate::models::AuthTokens)) {
    let mut tokens = snapshot().tokens;
    change(&mut tokens);
    write_active_auth_json(&AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(tokens),
        last_refresh: None,
        extra: Default::default(),
    })
    .unwrap();
}

pub(super) fn rotate_live_auth() {
    write_live_tokens(|tokens| tokens.access_token = "rotated-not-a-real-token".into());
}

/// Registry and live auth agree, but the token route no longer matches the
/// configured account route, so no request could be built.
pub(super) fn mismatch_registry_route() {
    edit_account(|account| account.tokens.account_id = Some("mismatched-route".into()));
    write_live_tokens(|tokens| tokens.account_id = Some("mismatched-route".into()));
}

pub(super) fn settings() -> Settings {
    Settings {
        auto_reset_weekly_enabled: true,
        auto_reset_weekly_min_remaining_seconds: 86_400,
        ..Settings::default()
    }
}

pub(super) fn snapshot() -> AccountConfig {
    let mut account = TestAccountSpec {
        id: ACCOUNT_ID,
        email: "user@example.invalid",
        plan: "team",
        weekly_pct: Some(0.0),
        credits: 1,
        ..TestAccountSpec::default()
    }
    .build();
    account.account_id = "account-id".into();
    account.tokens.account_id = Some("account-id".into());
    account.last_weekly_reset_time = Some("2026-01-08T00:00:00Z".into());
    account.last_weekly_reset_after_seconds = Some(100_000);
    account
}

/// Distinct credentials, including the refresh token, so registry
/// de-duplication keeps it as a separate account.
pub(super) fn other_account() -> AccountConfig {
    let mut account = snapshot();
    account.id = "other@example.invalid:other-route".into();
    account.email = "other@example.invalid".into();
    account.account_id = "other-route".into();
    account.tokens.account_id = Some("other-route".into());
    account.tokens.access_token = "other-not-a-real-token".into();
    account.tokens.refresh_token = Some("other-refresh-not-real".into());
    account.last_weekly_percentage = Some(80.0);
    account
}
