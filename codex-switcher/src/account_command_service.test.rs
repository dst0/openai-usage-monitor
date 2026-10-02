use super::{AccountCommandService, CONFIG_LOCK_WAIT};
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{accounts_json_path, switcher_lock_path, REGISTRY_BUSY};
use std::fs::{self, File, OpenOptions};
use std::time::Instant;

fn populated(label: &str) -> TestEnv {
    let env = TestEnv::new(label);
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
    env
}

fn hold_switcher_lock() -> File {
    let holder = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(switcher_lock_path())
        .unwrap();
    fs2::FileExt::lock_exclusive(&holder).unwrap();
    holder
}

/// `configure` bounds its own lock waits, with no budget set by its caller:
/// a write and a plain listing both report the registry busy and save nothing.
#[test]
fn configure_reports_a_held_registry_busy_on_its_own_budget() {
    let env = populated("configure_busy");
    let before = fs::read(accounts_json_path()).unwrap();
    let holder = hold_switcher_lock();

    let started = Instant::now();
    let write = AccountCommandService::configure(
        Some(true),
        None,
        None,
        None,
        Some(true),
        Some(4),
        Some(false),
    );
    let waited = started.elapsed();
    let listing = AccountCommandService::configure(None, None, None, None, None, None, None);
    drop(holder);

    assert_eq!(write, Err(REGISTRY_BUSY.to_string()));
    assert_eq!(
        listing,
        Err(REGISTRY_BUSY.to_string()),
        "busy must not print defaults"
    );
    assert!(waited >= CONFIG_LOCK_WAIT, "waited only {waited:?}");
    assert!(waited < CONFIG_LOCK_WAIT * 10, "waited {waited:?}");
    assert_eq!(fs::read(accounts_json_path()).unwrap(), before);
    drop(env);
}

#[test]
fn configure_saves_and_lists_when_the_registry_is_free() {
    let env = populated("configure_free");
    AccountCommandService::configure(None, None, None, None, None, None, Some(false)).unwrap();
    assert!(
        !crate::storage::load_accounts()
            .unwrap()
            .settings
            .preserve_window_bounds_on_restart
    );
    AccountCommandService::configure(None, None, None, None, None, None, None).unwrap();
    drop(env);
}
