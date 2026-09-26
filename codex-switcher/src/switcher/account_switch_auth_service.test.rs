use super::AccountSwitchAuthService;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::models::{AuthJson, AuthTokens};
use crate::storage::{
    compare_and_write_active_auth_json_for_switch, load_accounts, read_active_auth_json,
    update_accounts_atomically, write_active_auth_json,
};
use std::cell::Cell;

#[test]
fn target_auth_drops_prior_api_key_and_account_bound_token_extensions() {
    let mut previous = AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: Some("synthetic-prior-key".into()),
        tokens: Some(AuthTokens {
            access_token: "synthetic-prior-access".into(),
            refresh_token: Some("synthetic-prior-refresh".into()),
            id_token: None,
            account_id: Some("prior-workspace".into()),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    };
    previous.extra.insert(
        "desktop_extension".into(),
        serde_json::json!({"version": 1}),
    );
    previous
        .tokens
        .as_mut()
        .unwrap()
        .extra
        .insert("prior_only".into(), serde_json::json!(true));
    let target = TestAccountSpec {
        id: "next",
        email: "next@example.test",
        plan: "team",
        sprint_pct: 90.0,
        ..TestAccountSpec::default()
    }
    .build();

    let committed = AccountSwitchAuthService::prepare_replacement(&previous, &target);

    assert_eq!(committed.auth_mode.as_deref(), Some("chatgpt"));
    assert!(committed.openai_api_key.is_none());
    assert_eq!(committed.tokens, Some(target.tokens));
    assert_eq!(committed.extra, previous.extra);
}

#[test]
fn stale_target_rotation_before_auth_write_preserves_prior_auth() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_stale_post_stop_target");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "team",
                sprint_pct: 10.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let mut stale = load_accounts().unwrap();
    let selected = stale.accounts[1].clone();
    let previous = read_active_auth_json().unwrap();
    update_accounts_atomically(|fresh| {
        fresh.accounts[1].tokens.refresh_token = Some("synthetic-rotated-refresh".into());
        Ok(())
    })
    .unwrap();

    let result =
        AccountSwitchAuthService::replace(&selected, false, Some(previous.clone()), &mut stale);

    assert!(
        result.is_err_and(|error| error.contains("Selected account credentials changed")),
        "stale target must be rejected before auth write"
    );
    assert_eq!(read_active_auth_json().unwrap(), previous);
    drop(env);
}

#[test]
fn prewrite_failure_relaunches_only_with_exact_prior_auth_and_registry() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_prewrite_relaunch_guard");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "team",
                sprint_pct: 10.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let registry = load_accounts().unwrap();
    let previous = read_active_auth_json().unwrap();
    let prior_id = registry.active_account_id.as_deref();
    let launched = Cell::new(false);

    let result = AccountSwitchAuthService::abort_before_auth_write_with(
        true,
        &previous,
        prior_id,
        "synthetic target changed".into(),
        |error| {
            launched.set(true);
            error
        },
    );
    assert!(launched.get());
    assert!(result.contains("target changed"));

    let mut external = previous.clone();
    external
        .extra
        .insert("external_extension".into(), serde_json::json!(true));
    write_active_auth_json(&external).unwrap();
    launched.set(false);
    let result = AccountSwitchAuthService::abort_before_auth_write_with(
        true,
        &previous,
        prior_id,
        "synthetic target changed".into(),
        |error| {
            launched.set(true);
            error
        },
    );
    assert!(!launched.get());
    assert!(result.contains("Desktop remains stopped"));
    assert_eq!(read_active_auth_json().unwrap(), external);
    drop(env);
}

#[test]
fn failed_commit_clears_verified_prior_intent_before_desktop_relaunch() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_commit_rollback_order");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "team",
                sprint_pct: 10.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let registry = load_accounts().unwrap();
    let previous_id = registry.active_account_id.as_deref();
    let target = registry.accounts[1].clone();
    let previous = read_active_auth_json().unwrap();
    let committed = AccountSwitchAuthService::prepare_replacement(&previous, &target);
    super::DirectSwitchJournal::begin(
        env.home(),
        previous_id,
        &target,
        Some(&previous),
        &committed,
    )
    .unwrap();
    write_active_auth_json(&committed).unwrap();
    update_accounts_atomically(|fresh| {
        fresh.accounts[1].tokens.refresh_token = Some("synthetic-rotated-refresh".into());
        Ok(())
    })
    .unwrap();
    let commit_error =
        super::super::account_switch_commit_service::AccountSwitchCommitService::commit(
            &target,
            previous_id,
        )
        .unwrap_err();
    let relaunched = Cell::new(false);

    let error = AccountSwitchAuthService::rollback_after_commit_failure_with(
        Some(&previous),
        &committed,
        true,
        commit_error,
        |expected, committed, error| {
            compare_and_write_active_auth_json_for_switch(committed, expected.unwrap(), || {
                Ok(false)
            })
            .unwrap();
            error
        },
        |home| super::DirectSwitchJournal::reconcile_with(home, || Ok(false), false),
        |error| {
            assert!(
                std::fs::symlink_metadata(env.home().join("direct-switch-journal.json"))
                    .is_err_and(|missing| missing.kind() == std::io::ErrorKind::NotFound)
            );
            assert_eq!(read_active_auth_json().unwrap(), previous);
            assert_eq!(
                load_accounts().unwrap().active_account_id.as_deref(),
                previous_id
            );
            relaunched.set(true);
            error
        },
    );

    assert!(relaunched.get());
    assert!(error.contains("Selected account credentials changed"));
    drop(env);
}

#[test]
fn failed_commit_never_relaunches_if_rollback_or_readback_is_uncertain() {
    for mode in ["rollback_failed", "readback_changed"] {
        let env = crate::distribution::test_helper::TestEnv::new(mode);
        env.populate(
            vec![
                TestAccountSpec {
                    id: "old",
                    email: "old@example.test",
                    plan: "team",
                    sprint_pct: 10.0,
                    ..TestAccountSpec::default()
                }
                .build(),
                TestAccountSpec {
                    id: "next",
                    email: "next@example.test",
                    plan: "team",
                    sprint_pct: 80.0,
                    ..TestAccountSpec::default()
                }
                .build(),
            ],
            Some("old"),
            None,
        );
        let registry = load_accounts().unwrap();
        let target = registry.accounts[1].clone();
        let previous = read_active_auth_json().unwrap();
        let committed = AccountSwitchAuthService::prepare_replacement(&previous, &target);
        super::DirectSwitchJournal::begin(
            env.home(),
            registry.active_account_id.as_deref(),
            &target,
            Some(&previous),
            &committed,
        )
        .unwrap();
        write_active_auth_json(&committed).unwrap();
        let relaunched = Cell::new(false);

        let error = AccountSwitchAuthService::rollback_after_commit_failure_with(
            Some(&previous),
            &committed,
            true,
            "synthetic registry conflict".into(),
            |expected, committed, error| {
                if mode == "readback_changed" {
                    compare_and_write_active_auth_json_for_switch(
                        committed,
                        expected.unwrap(),
                        || Ok(false),
                    )
                    .unwrap();
                    let mut changed = expected.unwrap().clone();
                    changed
                        .extra
                        .insert("external_extension".into(), serde_json::json!(true));
                    write_active_auth_json(&changed).unwrap();
                    error
                } else {
                    format!("{error}; synthetic auth rollback failed")
                }
            },
            |home| super::DirectSwitchJournal::reconcile_with(home, || Ok(false), false),
            |error| {
                relaunched.set(true);
                error
            },
        );

        assert!(!relaunched.get(), "{mode}");
        assert!(error.contains("intent is unresolved"), "{mode}: {error}");
        assert!(
            std::fs::symlink_metadata(env.home().join("direct-switch-journal.json")).is_ok(),
            "{mode} must retain the journal"
        );
        drop(env);
    }
}
