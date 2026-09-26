use super::{read_active_auth_json, read_active_auth_json_with_hook, write_active_auth_json};
use crate::distribution::test_helper::TestEnv;
use crate::models::{AuthJson, AuthTokens};
use std::os::unix::fs::{symlink, PermissionsExt};

fn synthetic_auth() -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: "synthetic-access".into(),
            refresh_token: Some("synthetic-refresh".into()),
            id_token: None,
            account_id: Some("synthetic-workspace".into()),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    }
}

#[test]
fn active_auth_reader_refuses_a_symlink_even_when_target_is_valid() {
    let env = TestEnv::new("auth_read_symlink");
    let victim = env.home().join("synthetic-victim.json");
    std::fs::write(&victim, serde_json::to_vec(&synthetic_auth()).unwrap()).unwrap();
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&victim, crate::storage::auth_json_path()).unwrap();

    assert!(read_active_auth_json().is_err());
    assert_eq!(
        std::fs::read(&victim).unwrap(),
        serde_json::to_vec(&synthetic_auth()).unwrap()
    );
    drop(env);
}

#[test]
fn active_auth_reader_rejects_path_replacement_after_open() {
    let env = TestEnv::new("auth_read_path_race");
    write_active_auth_json(&synthetic_auth()).unwrap();
    let mut replacement_auth = synthetic_auth();
    replacement_auth.tokens.as_mut().unwrap().refresh_token = Some("synthetic-new-refresh".into());
    let replacement = env.home().join("replacement.json");
    std::fs::write(&replacement, serde_json::to_vec(&replacement_auth).unwrap()).unwrap();
    std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600)).unwrap();

    let result = read_active_auth_json_with_hook(|| {
        std::fs::rename(&replacement, crate::storage::auth_json_path()).unwrap();
    });

    assert!(result.is_err());
    assert_eq!(read_active_auth_json().unwrap(), replacement_auth);
    drop(env);
}

#[test]
fn active_auth_reader_rejects_world_readable_credentials() {
    let env = TestEnv::new("auth_read_permissions");
    write_active_auth_json(&synthetic_auth()).unwrap();
    std::fs::set_permissions(
        crate::storage::auth_json_path(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    assert!(read_active_auth_json().is_err());
    drop(env);
}
