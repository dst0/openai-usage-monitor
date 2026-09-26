use super::{DirectSwitchJournal, DirectSwitchJournalStore};
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::models::{AccountConfig, AuthJson};
use crate::storage::{load_accounts, read_active_auth_json, write_active_auth_json};
use std::os::unix::fs::{symlink, PermissionsExt};

fn fixture(label: &str) -> (TestEnv, AccountConfig, AuthJson, AuthJson) {
    let env = TestEnv::new(label);
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
                "team",
                10.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.test",
                "team",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        None,
    );
    let registry = load_accounts().unwrap();
    let target = registry.accounts[1].clone();
    let previous = read_active_auth_json().unwrap();
    let mut committed = previous.clone();
    committed.tokens = Some(target.tokens.clone());
    committed.last_refresh = Some("2026-09-26T00:00:00Z".into());
    (env, target, previous, committed)
}

fn restore_home(env: TestEnv, prior_home: Option<std::ffi::OsString>) {
    drop(env);
    if let Some(prior_home) = prior_home {
        std::env::set_var("CODEX_HOME", prior_home);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
}

#[test]
fn crash_after_auth_write_reconciles_fresh_registry_before_next_switch() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let (env, target, previous, committed) = fixture("direct_switch_auth_crash");
    let previous_id = load_accounts().unwrap().active_account_id.unwrap();
    crate::switcher::create_direct_switch_intent_for_test(
        env.home(),
        Some(&previous_id),
        &target,
        Some(&previous),
        &committed,
    )
    .unwrap();
    let bytes = std::fs::read(env.home().join("direct-switch-journal.json")).unwrap();
    assert!(!bytes
        .windows(b"tok_next".len())
        .any(|window| window == b"tok_next"));
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some(previous_id.as_str())
    );

    write_active_auth_json(&committed).unwrap();
    DirectSwitchJournal::reconcile_with(env.home(), || Ok(false), true).unwrap();

    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some(target.id.as_str())
    );
    assert_eq!(read_active_auth_json().unwrap(), committed);
    assert!(DirectSwitchJournalStore::load(env.home())
        .unwrap()
        .is_none());
    restore_home(env, prior_home);
}

#[test]
fn crash_before_auth_write_clears_intent_only_with_exact_prior_state() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let (env, target, previous, committed) = fixture("direct_switch_before_write_crash");
    let previous_id = load_accounts().unwrap().active_account_id.unwrap();
    DirectSwitchJournal::begin(
        env.home(),
        Some(&previous_id),
        &target,
        Some(&previous),
        &committed,
    )
    .unwrap();

    DirectSwitchJournal::reconcile_with(env.home(), || Ok(false), true).unwrap();

    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some(previous_id.as_str())
    );
    assert_eq!(read_active_auth_json().unwrap(), previous);
    assert!(DirectSwitchJournalStore::load(env.home())
        .unwrap()
        .is_none());
    restore_home(env, prior_home);
}

#[test]
fn changed_auth_extension_retains_intent_and_blocks_registry_commit() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let (env, target, previous, committed) = fixture("direct_switch_changed_extension");
    let previous_id = load_accounts().unwrap().active_account_id.unwrap();
    DirectSwitchJournal::begin(
        env.home(),
        Some(&previous_id),
        &target,
        Some(&previous),
        &committed,
    )
    .unwrap();
    let mut external = committed.clone();
    external
        .extra
        .insert("external_extension".into(), serde_json::json!(true));
    write_active_auth_json(&external).unwrap();

    assert!(DirectSwitchJournal::reconcile_with(env.home(), || Ok(false), true).is_err());
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some(previous_id.as_str())
    );
    assert_eq!(read_active_auth_json().unwrap(), external);
    assert!(DirectSwitchJournalStore::load(env.home())
        .unwrap()
        .is_some());
    restore_home(env, prior_home);
}

#[test]
fn unsafe_intent_symlink_and_mode_fail_closed() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let (env, target, previous, committed) = fixture("direct_switch_unsafe_intent");
    let previous_id = load_accounts().unwrap().active_account_id.unwrap();
    let path = env.home().join("direct-switch-journal.json");
    let victim = env.home().join("synthetic-victim");
    std::fs::write(&victim, b"keep synthetic data").unwrap();
    symlink(&victim, &path).unwrap();
    assert!(DirectSwitchJournal::begin(
        env.home(),
        Some(&previous_id),
        &target,
        Some(&previous),
        &committed
    )
    .is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep synthetic data");
    assert!(DirectSwitchJournalStore::load(env.home()).is_err());
    std::fs::remove_file(&path).unwrap();
    DirectSwitchJournal::begin(
        env.home(),
        Some(&previous_id),
        &target,
        Some(&previous),
        &committed,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(DirectSwitchJournalStore::load(env.home()).is_err());
    restore_home(env, prior_home);
}
