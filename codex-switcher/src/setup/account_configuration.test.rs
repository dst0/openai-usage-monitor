use super::set_config_preserve_window_bounds_with_hook;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{load_accounts, save_accounts};

#[test]
fn unrelated_setting_change_keeps_newer_auto_switch_disable_and_credentials() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("config_concurrent_auth_and_disable");
    env.populate(
        vec![TestAccountSpec {
            id: "main",
            email: "owner@example.test",
            plan: "team",
            sprint_pct: 100.0,
            ..TestAccountSpec::default()
        }
        .build()],
        Some("main"),
        None,
    );

    set_config_preserve_window_bounds_with_hook(true, || {
        let mut newer = load_accounts()?;
        newer.settings.auto_switch_enabled = false;
        newer.accounts[0].tokens.refresh_token = Some("fresh-refresh".into());
        save_accounts(&newer)
    })
    .unwrap();

    let saved = load_accounts().unwrap();
    assert!(saved.settings.preserve_window_bounds_on_restart);
    assert!(!saved.settings.auto_switch_enabled);
    assert_eq!(
        saved.accounts[0].tokens.refresh_token.as_deref(),
        Some("fresh-refresh")
    );
    drop(env);
    std::env::remove_var("CODEX_HOME");
}
