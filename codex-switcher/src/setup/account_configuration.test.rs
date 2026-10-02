use super::set_config_preserve_window_bounds_with_hook;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{load_accounts, save_accounts};

#[test]
fn unrelated_setting_change_keeps_newer_auto_switch_disable_and_credentials() {
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
}

mod busy_registry {
    use super::super::{
        set_config_auto_reset_weekly, set_config_auto_switch_enabled, sync_status_cache_or_defer,
    };
    use crate::distribution::test_account_spec::TestAccountSpec;
    use crate::distribution::test_helper::TestEnv;
    use crate::storage::{
        codex_home, load_accounts, status_json_path, switcher_lock_path, with_lock_wait_budget,
        REGISTRY_BUSY,
    };
    use std::fs::{self, File, OpenOptions};
    use std::time::{Duration, Instant};

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

    /// Another holder of the switcher lock, as a stuck Monitor process would be.
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

    fn staging_files() -> Vec<String> {
        fs::read_dir(codex_home())
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.contains(".tmp"))
            .collect()
    }

    #[test]
    fn a_held_registry_lock_fails_the_write_at_its_deadline_and_changes_nothing() {
        let env = populated("config_busy_registry");
        let before = fs::read(crate::storage::accounts_json_path()).unwrap();
        let holder = hold_switcher_lock();

        let started = Instant::now();
        let result = with_lock_wait_budget(Duration::from_millis(200), || {
            set_config_auto_switch_enabled(false)
        });
        let waited = started.elapsed();
        drop(holder);

        assert_eq!(result, Err(REGISTRY_BUSY.to_string()));
        assert!(
            waited >= Duration::from_millis(200),
            "waited only {waited:?}"
        );
        assert!(waited < Duration::from_secs(5), "waited {waited:?}");
        assert_eq!(
            fs::read(crate::storage::accounts_json_path()).unwrap(),
            before
        );
        assert!(staging_files().is_empty(), "left {:?}", staging_files());
        drop(env);
    }

    #[test]
    fn a_busy_status_cache_sync_after_the_save_is_deferred_not_failed() {
        let env = populated("config_busy_status_sync");
        fs::write(status_json_path(), b"untouched").unwrap();
        let holder = hold_switcher_lock();

        let result = with_lock_wait_budget(Duration::from_millis(100), sync_status_cache_or_defer);
        drop(holder);

        assert_eq!(
            result,
            Ok(()),
            "the registry is saved, so the command succeeds"
        );
        assert_eq!(fs::read(status_json_path()).unwrap(), b"untouched");
        drop(env);
    }

    #[test]
    fn other_status_cache_failures_are_still_reported() {
        let env = populated("config_status_sync_failure");
        fs::write(status_json_path(), b"not json").unwrap();
        let result = with_lock_wait_budget(Duration::from_millis(100), sync_status_cache_or_defer);
        assert!(
            matches!(&result, Err(error) if error != REGISTRY_BUSY),
            "{result:?}"
        );
        drop(env);
    }

    #[test]
    fn weekly_reset_keeps_the_saved_value_it_was_not_given() {
        let env = populated("config_weekly_merge");
        set_config_auto_reset_weekly(Some(true), Some(24)).unwrap();
        set_config_auto_reset_weekly(None, Some(5)).unwrap();
        let settings = load_accounts().unwrap().settings;
        assert!(
            settings.auto_reset_weekly_enabled,
            "enabled must stay saved"
        );
        assert_eq!(settings.auto_reset_weekly_min_remaining_seconds, 5 * 3600);

        set_config_auto_reset_weekly(Some(false), None).unwrap();
        let settings = load_accounts().unwrap().settings;
        assert!(!settings.auto_reset_weekly_enabled);
        assert_eq!(settings.auto_reset_weekly_min_remaining_seconds, 5 * 3600);

        assert!(set_config_auto_reset_weekly(Some(true), Some(168)).is_err());
        let settings = load_accounts().unwrap().settings;
        assert!(
            !settings.auto_reset_weekly_enabled,
            "a rejected threshold saves nothing"
        );
        drop(env);
    }
}
