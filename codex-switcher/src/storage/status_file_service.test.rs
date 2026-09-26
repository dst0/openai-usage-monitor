use super::{read_status_file, sync_settings_to_status_file, write_status_file};
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::models::StatusFile;
use crate::storage::{load_accounts, save_accounts};

fn status() -> StatusFile {
    StatusFile {
        timestamp: "synthetic".into(),
        active_account_id: Some("main".into()),
        active_email: None,
        active_plan: None,
        five_hour_percentage: 100.0,
        weekly_percentage: None,
        weekly_reset_time: None,
        weekly_reset_after_seconds: None,
        reset_time: None,
        reset_after_seconds: None,
        credits: 0,
        auto_switch_enabled: true,
        auto_switch_business_only: false,
        auto_switch_business_priority: true,
        auto_reset_weekly_enabled: false,
        auto_reset_weekly_min_remaining_seconds: 0,
        auto_reset_state: "disabled".into(),
        auto_reset_reason: None,
        auto_reset_last_event_at: None,
        plan_multiplier: 1.0,
        accounts: Vec::new(),
    }
}

#[test]
fn stale_status_writers_cannot_reenable_auto_switch_after_registry_disable() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("status_cache_disable_race");
    env.populate(
        vec![make_account(
            "main",
            None,
            "owner@example.test",
            "team",
            100.0,
            None,
            0,
            None,
            None,
        )],
        Some("main"),
        None,
    );
    let stale_status = status();
    write_status_file(&stale_status).unwrap();
    let mut latest = load_accounts().unwrap();
    latest.settings.auto_switch_enabled = false;
    latest.settings.auto_switch_business_priority = false;
    save_accounts(&latest).unwrap();

    sync_settings_to_status_file().unwrap();
    assert!(!read_status_file().unwrap().auto_switch_enabled);
    write_status_file(&stale_status).unwrap();
    let observed = read_status_file().unwrap();
    assert!(!observed.auto_switch_enabled);
    assert!(!observed.auto_switch_business_priority);
    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn status_writer_never_follows_a_preexisting_temporary_symlink() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("status_temp_symlink");
    env.populate(
        vec![make_account(
            "main",
            None,
            "owner@example.test",
            "team",
            100.0,
            None,
            0,
            None,
            None,
        )],
        Some("main"),
        None,
    );
    let path = crate::storage::status_json_path();
    let legacy_temp = path.with_extension(format!("{}.tmp.json", std::process::id()));
    let victim = env.home().join("synthetic-victim.txt");
    std::fs::write(&victim, b"keep synthetic data").unwrap();
    std::os::unix::fs::symlink(&victim, &legacy_temp).unwrap();

    write_status_file(&status()).unwrap();
    let unchanged = std::fs::read(&victim).unwrap() == b"keep synthetic data";
    assert!(crate::storage::status_json_path().is_file());
    assert!(std::fs::symlink_metadata(&legacy_temp)
        .unwrap()
        .file_type()
        .is_symlink());
    std::fs::remove_file(&legacy_temp).unwrap();
    assert!(
        unchanged,
        "status write followed an existing temporary symlink"
    );
    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn settings_sync_leaves_a_missing_status_cache_for_the_daemon() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("status_cache_missing");
    env.populate(
        vec![make_account(
            "main",
            None,
            "owner@example.test",
            "team",
            100.0,
            None,
            0,
            None,
            None,
        )],
        Some("main"),
        None,
    );
    let path = crate::storage::status_json_path();
    assert!(!path.exists());
    sync_settings_to_status_file().unwrap();
    assert!(
        !path.exists(),
        "settings sync created an incomplete quota snapshot"
    );
    drop(env);
    std::env::remove_var("CODEX_HOME");
}
