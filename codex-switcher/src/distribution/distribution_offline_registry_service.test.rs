use super::*;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;

#[test]
fn commit_preserves_concurrent_settings_and_unrelated_account() {
    let env = TestEnv::new("offline_registry_field_commit");
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
    let old = storage::load_accounts().unwrap();
    let target = old.accounts[1].clone();
    storage::update_accounts_atomically(|fresh| {
        fresh.settings.auto_switch_enabled = false;
        fresh.accounts[0].priority = 91;
        Ok(())
    })
    .unwrap();

    let committed =
        DistributionOfflineRegistryService::commit(&target, old.active_account_id.as_deref())
            .unwrap();

    assert_eq!(
        committed.active_account_id.as_deref(),
        Some(target.id.as_str())
    );
    assert!(!committed.settings.auto_switch_enabled);
    assert_eq!(committed.accounts[0].priority, 91);
}

#[test]
fn commit_refuses_a_reauthenticated_target() {
    let env = TestEnv::new("offline_registry_target_changed");
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
    let old = storage::load_accounts().unwrap();
    let target = old.accounts[1].clone();
    storage::update_accounts_atomically(|fresh| {
        fresh.accounts[1].tokens.refresh_token = Some("synthetic-relogin".into());
        Ok(())
    })
    .unwrap();

    assert!(
        DistributionOfflineRegistryService::commit(&target, old.active_account_id.as_deref())
            .is_err()
    );
    assert_eq!(
        storage::load_accounts().unwrap().active_account_id,
        old.active_account_id
    );
}
