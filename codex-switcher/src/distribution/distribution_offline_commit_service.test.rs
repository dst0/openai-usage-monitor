use super::*;
use crate::distribution::mock_app_lifecycle::MockAppLifecycle;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{self, load_accounts, save_accounts, write_active_auth_json};

#[test]
fn marker_failure_rollback_preserves_concurrent_registry_changes() {
    let env = TestEnv::new("offline_marker_concurrent_registry");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 90.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let before_accounts = load_accounts().unwrap();
    let previous_auth = storage::read_active_auth_json().unwrap();
    let mut committed_auth = previous_auth.clone();
    committed_auth.tokens = Some(before_accounts.accounts[1].tokens.clone());
    write_active_auth_json(&committed_auth).unwrap();
    let mut accounts = before_accounts.clone();
    accounts.active_account_id = Some(before_accounts.accounts[1].id.clone());
    save_accounts(&accounts).unwrap();
    storage::update_accounts_atomically(|fresh| {
        fresh.settings.auto_switch_enabled = false;
        fresh.accounts[0].priority = 77;
        Ok(())
    })
    .unwrap();
    let lifecycle = MockAppLifecycle::new(false);
    let logger = DistributionAuditLogger::default();

    let message = DistributionOfflineCommitService::new(&lifecycle, &logger, "test")
        .rollback_marker_failure(
            env.home(),
            &mut accounts,
            &before_accounts,
            Some(&(previous_auth, committed_auth)),
            (None, &DesktopAppSession::new("next")),
            "synthetic marker failure".into(),
        );

    assert!(message.contains("rolled back"), "{message}");
    let latest = load_accounts().unwrap();
    assert_eq!(latest.active_account_id, before_accounts.active_account_id);
    assert!(!latest.settings.auto_switch_enabled);
    assert_eq!(latest.accounts[0].priority, 77);
}

#[test]
fn marker_failure_cannot_clear_journal_after_previous_account_relogin() {
    let env = TestEnv::new("offline_marker_previous_relogin");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 90.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let before_accounts = load_accounts().unwrap();
    let previous_auth = storage::read_active_auth_json().unwrap();
    let mut committed_auth = previous_auth.clone();
    committed_auth.tokens = Some(before_accounts.accounts[1].tokens.clone());
    write_active_auth_json(&committed_auth).unwrap();
    let mut accounts = before_accounts.clone();
    accounts.active_account_id = Some(before_accounts.accounts[1].id.clone());
    save_accounts(&accounts).unwrap();
    storage::update_accounts_atomically(|fresh| {
        fresh.accounts[0].tokens.refresh_token = Some("synthetic-relogin".into());
        Ok(())
    })
    .unwrap();
    DistributionJournal::create(
        env.home(),
        "prior-switch",
        "user",
        "synthetic",
        Some("next"),
        Some("next"),
    )
    .unwrap();
    let lifecycle = MockAppLifecycle::new(false);
    let logger = DistributionAuditLogger::default();

    let message = DistributionOfflineCommitService::new(&lifecycle, &logger, "test")
        .rollback_marker_failure(
            env.home(),
            &mut accounts,
            &before_accounts,
            Some(&(previous_auth, committed_auth)),
            (None, &DesktopAppSession::new("next")),
            "synthetic marker failure".into(),
        );

    assert!(message.contains("unverified"), "{message}");
    assert!(DistributionJournal::journal_path(env.home()).exists());
}

#[test]
fn post_rename_marker_failure_restores_marker_before_journal_cleanup() {
    let env = TestEnv::new("offline_marker_postrename");
    env.populate(vec![], None, Some("previous"));
    let path = env.home().join("desktop-app-session.json");
    let previous = DesktopAppSession::load_checked(&path).unwrap().unwrap();
    let attempted = DesktopAppSession::new("attempted");
    attempted.save(&path).unwrap();
    DistributionJournal::create(env.home(), "marker-rollback", "user", "test", None, None).unwrap();
    let mut accounts = load_accounts().unwrap();
    let before_accounts = accounts.clone();
    let lifecycle = MockAppLifecycle::new(false);
    let logger = DistributionAuditLogger::default();

    let message = DistributionOfflineCommitService::new(&lifecycle, &logger, "test")
        .rollback_marker_failure(
            env.home(),
            &mut accounts,
            &before_accounts,
            None,
            (Some(&previous), &attempted),
            "synthetic post-rename failure".into(),
        );

    assert!(message.contains("rolled back"), "{message}");
    assert_eq!(
        DesktopAppSession::load_checked(&path).unwrap(),
        Some(previous)
    );
    assert!(!DistributionJournal::journal_path(env.home()).exists());
}

#[test]
fn changed_marker_keeps_journal_when_rollback_cannot_be_verified() {
    let env = TestEnv::new("offline_marker_changed");
    env.populate(vec![], None, Some("previous"));
    let path = env.home().join("desktop-app-session.json");
    let previous = DesktopAppSession::load_checked(&path).unwrap().unwrap();
    let attempted = DesktopAppSession::new("attempted");
    DesktopAppSession::new("newer").save(&path).unwrap();
    DistributionJournal::create(env.home(), "marker-changed", "user", "test", None, None).unwrap();
    let mut accounts = load_accounts().unwrap();
    let before_accounts = accounts.clone();
    let lifecycle = MockAppLifecycle::new(false);
    let logger = DistributionAuditLogger::default();

    let message = DistributionOfflineCommitService::new(&lifecycle, &logger, "test")
        .rollback_marker_failure(
            env.home(),
            &mut accounts,
            &before_accounts,
            None,
            (Some(&previous), &attempted),
            "synthetic post-rename failure".into(),
        );

    assert!(message.contains("marker rollback unverified"), "{message}");
    assert!(DistributionJournal::journal_path(env.home()).exists());
    assert_eq!(
        DesktopAppSession::load_checked(&path)
            .unwrap()
            .unwrap()
            .account_id,
        "newer"
    );
}

#[test]
fn full_offline_commit_rolls_back_auth_registry_and_marker_after_post_rename_error() {
    let env = TestEnv::new("offline_marker_full_postrename");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 90.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        Some("old"),
    );
    let marker_path = env.home().join("desktop-app-session.json");
    let before_marker = DesktopAppSession::load_checked(&marker_path).unwrap();
    let before_auth = storage::read_active_auth_json().unwrap();
    let mut accounts = load_accounts().unwrap();
    let before_accounts = accounts.clone();
    let old_id = accounts
        .accounts
        .iter()
        .find(|a| a.account_id == "old")
        .unwrap()
        .id
        .clone();
    let next_id = accounts
        .accounts
        .iter()
        .find(|a| a.account_id == "next")
        .unwrap()
        .id
        .clone();
    let plan = DistributionPlan {
        current_app_id: Some(old_id.clone()),
        current_cli_id: Some(old_id),
        target_app_id: Some(next_id.clone()),
        target_cli_id: Some(next_id.clone()),
        app_switch_needed: true,
        cli_switch_needed: true,
        restart_required: false,
        decision_reason: "synthetic_postrename".into(),
        evaluated_candidates: Vec::new(),
    };
    let mut journal = DistributionJournal::create(
        env.home(),
        "full-marker-rollback",
        "user",
        "test",
        Some(&next_id),
        Some(&next_id),
    )
    .unwrap();
    let lifecycle = MockAppLifecycle::new(false);
    let logger = DistributionAuditLogger::default();

    let result = DistributionOfflineCommitService::new(&lifecycle, &logger, "full-marker-rollback")
        .run_with_marker_writer(
            env.home(),
            &mut journal,
            &plan,
            &DistributionRequest::user("synthetic_postrename"),
            &mut accounts,
            |marker, path| {
                marker
                    .save_with_post_rename(path, |_| Err("synthetic post-rename sync error".into()))
            },
        );

    assert!(result.unwrap_err().contains("rolled back"));
    assert_eq!(storage::read_active_auth_json().unwrap(), before_auth);
    assert_eq!(
        load_accounts().unwrap().active_account_id,
        before_accounts.active_account_id
    );
    assert_eq!(
        DesktopAppSession::load_checked(&marker_path).unwrap(),
        before_marker
    );
    assert!(!DistributionJournal::journal_path(env.home()).exists());
}
