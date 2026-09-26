use super::ActiveAuthBindingService;
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::storage::{load_accounts, read_active_auth_json, write_active_auth_json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

fn jwt(email: &str, generation: &str) -> String {
    let claims = URL_SAFE_NO_PAD.encode(format!(r#"{{"email":"{email}"}}"#));
    format!("header.{claims}.signature-{generation}")
}

#[test]
fn same_email_distinct_provider_token_aliases_cannot_bind_recovery() {
    let env = TestEnv::new("recovery_cross_provider_alias");
    let first = make_account(
        "provider-a",
        None,
        "owner@example.test",
        "plus",
        20.0,
        None,
        0,
        None,
        None,
    );
    let mut second = make_account(
        "provider-b",
        None,
        "owner@example.test",
        "team",
        80.0,
        None,
        0,
        None,
        None,
    );
    second.tokens.access_token = jwt("owner@example.test", "provider-b");
    second.tokens.id_token = Some(jwt("owner@example.test", "provider-b-id"));
    env.populate(
        vec![first, second.clone()],
        Some("provider-a"),
        Some("provider-a"),
    );
    let original = read_active_auth_json().unwrap();
    for alias in ["access", "refresh", "both", "id"] {
        let mut auth = original.clone();
        let tokens = auth.tokens.as_mut().unwrap();
        tokens.access_token = jwt("owner@example.test", "provider-a-new");
        tokens.refresh_token = Some("refresh-provider-a-new".into());
        if alias == "access" || alias == "both" {
            tokens.access_token = second.tokens.access_token.clone();
        }
        if alias == "refresh" || alias == "both" {
            tokens.refresh_token = second.tokens.refresh_token.clone();
        }
        if alias == "id" {
            tokens.id_token = second.tokens.id_token.clone();
        }
        assert_eq!(tokens.account_id.as_deref(), Some("provider-a"));
        write_active_auth_json(&auth).unwrap();
        assert_eq!(ActiveAuthBindingService::current(), None, "{alias}");
    }
}

#[test]
fn same_email_distinct_provider_without_alias_binds_unique_recovery_account() {
    let env = TestEnv::new("recovery_cross_provider_unique");
    let first = make_account(
        "provider-a",
        None,
        "owner@example.test",
        "plus",
        20.0,
        None,
        0,
        None,
        None,
    );
    let second = make_account(
        "provider-b",
        None,
        "owner@example.test",
        "team",
        80.0,
        None,
        0,
        None,
        None,
    );
    env.populate(vec![first, second], Some("provider-a"), Some("provider-a"));
    let first_id = load_accounts().unwrap().active_account_id.unwrap();
    let mut auth = read_active_auth_json().unwrap();
    let tokens = auth.tokens.as_mut().unwrap();
    tokens.access_token = jwt("owner@example.test", "provider-a-new");
    tokens.refresh_token = Some("refresh-provider-a-new".into());
    write_active_auth_json(&auth).unwrap();

    assert_eq!(ActiveAuthBindingService::current(), Some(first_id));
}
