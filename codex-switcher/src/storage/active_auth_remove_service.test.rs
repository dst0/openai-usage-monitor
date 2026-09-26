use super::ActiveAuthRemoveService;
use crate::distribution::test_helper::TestEnv;
use crate::models::{AuthJson, AuthTokens};
use crate::storage::{auth_json_path, read_active_auth_json, write_active_auth_json};

fn auth(refresh: &str) -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: "synthetic-access".into(),
            refresh_token: Some(refresh.into()),
            id_token: None,
            account_id: Some("synthetic-workspace".into()),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    }
}

#[test]
fn first_auth_rollback_detects_in_place_change_before_unlink() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = TestEnv::new("auth_remove_in_place_race");
    let committed = auth("committed");
    let external = auth("external");
    write_active_auth_json(&committed).unwrap();

    let result = ActiveAuthRemoveService::remove_if_matches_with_hook(
        &committed,
        || Ok(false),
        || std::fs::write(auth_json_path(), serde_json::to_vec(&external).unwrap()).unwrap(),
    );

    assert!(result.is_err());
    assert_eq!(read_active_auth_json().unwrap(), external);
    drop(env);
    if let Some(prior_home) = prior_home {
        std::env::set_var("CODEX_HOME", prior_home);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
}
