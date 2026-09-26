use super::relogin_commit_service::ReloginCommitService;
use crate::models::{AccountConfig, AccountsFile, AuthJson};
use std::cell::{Cell, RefCell};

fn fixtures(active: bool) -> (AccountConfig, AccountsFile, AuthJson) {
    let original: AccountConfig = serde_json::from_value(serde_json::json!({
        "id": "owner@example.com:workspace-1",
        "email": "owner@example.com",
        "account_id": "workspace-1",
        "tokens": {"access_token": "old-access", "refresh_token": "old-refresh", "account_id": "workspace-1"}
    }))
    .unwrap();
    let mut updated = original.clone();
    updated.tokens.access_token = "new-access".into();
    updated.tokens.refresh_token = Some("new-refresh".into());
    let staged = AccountsFile {
        active_account_id: active.then(|| original.id.clone()),
        settings: Default::default(),
        accounts: vec![updated],
    };
    let auth = AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(original.tokens.clone()),
        last_refresh: None,
        extra: Default::default(),
    };
    (original, staged, auth)
}

#[test]
fn active_relogin_rejects_running_desktop_before_any_auth_or_registry_write() {
    let (original, staged, _) = fixtures(true);
    let writes = Cell::new(0);
    let result = ReloginCommitService::new(&original, &staged, &original.id).commit_with(
        || Ok(true),
        || panic!("auth must not be read while Desktop is active"),
        |_, _| {
            writes.set(writes.get() + 1);
            Ok(())
        },
        |_, _| {
            writes.set(writes.get() + 1);
            Ok(())
        },
    );
    assert!(result.unwrap_err().contains("close it"));
    assert_eq!(writes.get(), 0);
}

#[test]
fn active_relogin_commits_the_final_staged_tokens_to_both_files() {
    let (original, staged, initial_auth) = fixtures(true);
    let auth = RefCell::new(initial_auth);
    let saved = RefCell::new(None);
    ReloginCommitService::new(&original, &staged, &original.id)
        .commit_with(
            || Ok(false),
            || Ok(auth.borrow().clone()),
            |expected, replacement| {
                assert_eq!(auth.borrow().tokens, expected.tokens);
                *auth.borrow_mut() = replacement.clone();
                Ok(())
            },
            |file, expected_auth| {
                assert!(expected_auth.is_some());
                *saved.borrow_mut() = Some(file.clone());
                Ok(())
            },
        )
        .unwrap();
    let expected = &staged.accounts[0].tokens;
    assert_eq!(auth.borrow().tokens.as_ref(), Some(expected));
    assert_eq!(
        saved.borrow().as_ref().unwrap().accounts[0].tokens,
        *expected
    );
}

#[test]
fn active_relogin_preserves_unknown_token_metadata_in_auth_and_registry() {
    let (mut original, staged, mut auth) = fixtures(true);
    original
        .tokens
        .extra
        .insert("desktop_metadata".into(), serde_json::json!({"version": 3}));
    auth.tokens.as_mut().unwrap().extra = original.tokens.extra.clone();
    let live = RefCell::new(auth);
    let saved = RefCell::new(None);
    ReloginCommitService::new(&original, &staged, &original.id)
        .commit_with(
            || Ok(false),
            || Ok(live.borrow().clone()),
            |expected, replacement| {
                assert_eq!(live.borrow().extra, expected.extra);
                *live.borrow_mut() = replacement.clone();
                Ok(())
            },
            |file, expected| {
                assert!(expected.is_some());
                *saved.borrow_mut() = Some(file.clone());
                Ok(())
            },
        )
        .unwrap();
    let expected = original.tokens.extra;
    assert_eq!(live.borrow().tokens.as_ref().unwrap().extra, expected);
    assert_eq!(
        saved.borrow().as_ref().unwrap().accounts[0].tokens.extra,
        expected
    );
}

#[test]
fn active_relogin_aborts_if_live_auth_changes_before_commit() {
    let (original, staged, initial_auth) = fixtures(true);
    let mut changed_auth = initial_auth.clone();
    changed_auth.tokens.as_mut().unwrap().refresh_token = Some("rotated-elsewhere".into());
    let reads = Cell::new(0);
    let writes = Cell::new(0);
    let result = ReloginCommitService::new(&original, &staged, &original.id).commit_with(
        || Ok(false),
        || {
            let count = reads.get();
            reads.set(count + 1);
            Ok(if count == 0 {
                initial_auth.clone()
            } else {
                changed_auth.clone()
            })
        },
        |_, _| {
            writes.set(writes.get() + 1);
            Ok(())
        },
        |_, _| {
            writes.set(writes.get() + 1);
            Ok(())
        },
    );
    assert!(result.unwrap_err().contains("changed before"));
    assert_eq!(writes.get(), 0);
}

#[test]
fn active_relogin_must_not_overwrite_auth_changed_at_commit_boundary() {
    let (original, staged, initial_auth) = fixtures(true);
    let auth = RefCell::new(initial_auth);
    let wrote = Cell::new(false);
    let result = ReloginCommitService::new(&original, &staged, &original.id).commit_with(
        || Ok(false),
        || Ok(auth.borrow().clone()),
        |expected, replacement| {
            auth.borrow_mut().tokens.as_mut().unwrap().refresh_token =
                Some("external-rotation".into());
            if auth.borrow().tokens != expected.tokens {
                return Err("Shared credentials changed before re-login commit".into());
            }
            wrote.set(true);
            *auth.borrow_mut() = replacement.clone();
            Ok(())
        },
        |_, _| Ok(()),
    );
    assert!(result
        .unwrap_err()
        .contains("changed before re-login commit"));
    assert!(!wrote.get());
    assert_eq!(
        auth.borrow()
            .tokens
            .as_ref()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("external-rotation")
    );
}

#[test]
fn registry_save_failure_keeps_new_active_auth_instead_of_restoring_revoked_token() {
    let (original, staged, initial_auth) = fixtures(true);
    let auth = RefCell::new(initial_auth);
    let result = ReloginCommitService::new(&original, &staged, &original.id).commit_with(
        || Ok(false),
        || Ok(auth.borrow().clone()),
        |_, replacement| {
            *auth.borrow_mut() = replacement.clone();
            Ok(())
        },
        |_, _| Err("disk unavailable".into()),
    );
    assert!(result.unwrap_err().contains("registry save failed"));
    assert_eq!(
        auth.borrow().tokens.as_ref(),
        Some(&staged.accounts[0].tokens)
    );
}

#[test]
fn inactive_relogin_updates_registry_without_touching_desktop_auth() {
    let (original, staged, initial_auth) = fixtures(false);
    let saved = RefCell::new(None);
    ReloginCommitService::new(&original, &staged, &original.id)
        .commit_with(
            || panic!("inactive re-login does not require Desktop shutdown"),
            || panic!("inactive re-login does not read active auth"),
            |_, _| panic!("inactive re-login does not write active auth"),
            |file, expected_auth| {
                assert!(expected_auth.is_none());
                *saved.borrow_mut() = Some(file.clone());
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(
        saved.borrow().as_ref().unwrap().accounts[0]
            .tokens
            .refresh_token
            .as_deref(),
        Some("new-refresh")
    );
    assert_eq!(
        initial_auth.tokens.unwrap().refresh_token.as_deref(),
        Some("old-refresh")
    );
}

#[test]
fn active_relogin_refuses_different_workspace_even_with_token_continuity() {
    let (original, staged, mut auth) = fixtures(true);
    auth.tokens.as_mut().unwrap().account_id = Some("other-workspace".into());
    let result = ReloginCommitService::new(&original, &staged, &original.id).commit_with(
        || Ok(false),
        || Ok(auth.clone()),
        |_, _| panic!("mismatched account cannot be committed"),
        |_, _| panic!("mismatched account cannot be saved"),
    );
    assert!(result.unwrap_err().contains("identity does not match"));
}

#[test]
fn active_relogin_refuses_explicit_other_email_despite_old_refresh_overlap() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let (original, staged, mut auth) = fixtures(true);
    let payload = URL_SAFE_NO_PAD.encode(br#"{"email":"another@example.com"}"#);
    auth.tokens.as_mut().unwrap().id_token = Some(format!("header.{payload}.signature"));
    let result = ReloginCommitService::new(&original, &staged, &original.id).commit_with(
        || Ok(false),
        || Ok(auth.clone()),
        |_, _| panic!("conflicting identity cannot write auth"),
        |_, _| panic!("conflicting identity cannot write registry"),
    );
    assert!(result.unwrap_err().contains("user identity"));
}
