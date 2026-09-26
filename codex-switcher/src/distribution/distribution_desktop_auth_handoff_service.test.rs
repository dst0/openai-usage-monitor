use super::*;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{
    load_accounts, read_active_auth_json, update_accounts_atomically, write_active_auth_json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

#[test]
fn shutdown_token_handoff_preserves_concurrent_registry_changes() {
    let env = TestEnv::new("handoff_registry_concurrency");
    let mut old = TestAccountSpec {
        id: "old",
        email: "old@example.test",
        plan: "plus",
        ..TestAccountSpec::default()
    }
    .build();
    let claims = URL_SAFE_NO_PAD.encode(r#"{"email":"old@example.test"}"#);
    old.tokens.access_token = format!("header.{claims}.signature");
    env.populate(
        vec![
            old,
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
    let mut rotated = read_active_auth_json().unwrap();
    rotated.tokens.as_mut().unwrap().refresh_token = Some("rotated-after-shutdown".into());
    write_active_auth_json(&rotated).unwrap();
    let mut snapshot = load_accounts().unwrap();

    DistributionDesktopAuthHandoffService::preserve_after_stop_with_hook(
        &mut snapshot,
        "old@example.test:old",
        || {
            update_accounts_atomically(|registry| {
                registry.settings.auto_switch_enabled = false;
                registry
                    .accounts
                    .iter_mut()
                    .find(|a| a.account_id == "next")
                    .unwrap()
                    .priority = 77;
                Ok(())
            })?;
            Ok(())
        },
    )
    .unwrap();

    let saved = load_accounts().unwrap();
    assert!(!saved.settings.auto_switch_enabled);
    assert_eq!(
        saved
            .accounts
            .iter()
            .find(|a| a.account_id == "next")
            .unwrap()
            .priority,
        77
    );
    assert_eq!(
        saved
            .accounts
            .iter()
            .find(|a| a.account_id == "old")
            .unwrap()
            .tokens
            .refresh_token
            .as_deref(),
        Some("rotated-after-shutdown")
    );
}
