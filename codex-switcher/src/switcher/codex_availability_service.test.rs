use super::CodexAvailabilityService;
use crate::models::{AuthJson, AuthTokens};
use crate::storage::{
    compare_and_write_active_auth_json_for_switch, read_active_auth_json, write_active_auth_json,
};
use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;

#[test]
fn resume_does_not_launch_a_running_desktop() {
    let launched = Cell::new(false);
    CodexAvailabilityService::ensure_running_for_resume(
        || true,
        || {
            launched.set(true);
            Ok(vec![1])
        },
    )
    .unwrap();
    assert!(!launched.get());
}

#[test]
fn resume_launches_a_closed_desktop_once() {
    let launches = Cell::new(0);
    CodexAvailabilityService::ensure_running_for_resume(
        || false,
        || {
            launches.set(launches.get() + 1);
            Ok(vec![4242])
        },
    )
    .unwrap();
    assert_eq!(launches.get(), 1);
}

#[test]
fn resume_launch_failure_explains_how_to_start_desktop() {
    let error = CodexAvailabilityService::ensure_running_for_resume(
        || false,
        || Err("Codex app launch was requested, but no stable main process appeared".into()),
    )
    .unwrap_err();
    assert!(error.starts_with("Codex is not running and could not be started automatically"));
    assert!(error.contains("no stable main process appeared"));
    assert!(error.contains("open -a /Applications/ChatGPT.app"));
    assert!(error.contains("cxi resume"));
}

#[test]
fn resume_starts_desktop_only_for_an_eligible_user_task() {
    let user_task = "01a0d8a1-f49c-7cd1-8e5f-cbf327837b5d".to_string();
    let is_user = |id: &str| id == "01a0d8a1-f49c-7cd1-8e5f-cbf327837b5d";
    assert!(CodexAvailabilityService::resume_has_launchable_target(
        std::slice::from_ref(&user_task),
        is_user
    ));
    assert!(!CodexAvailabilityService::resume_has_launchable_target(
        &["garbage".to_string()],
        is_user
    ));
    assert!(!CodexAvailabilityService::resume_has_launchable_target(
        &[],
        is_user
    ));
}

fn old_auth() -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: "old-access".into(),
            refresh_token: Some("old-refresh".into()),
            id_token: None,
            account_id: Some("old-account".into()),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    }
}

fn committed_auth() -> AuthJson {
    let mut committed = old_auth();
    committed.tokens.as_mut().unwrap().access_token = "committed-synthetic-access".into();
    committed
}

#[test]
fn failed_auth_restore_never_relaunches_desktop() {
    let launched = Cell::new(false);
    let error = CodexAvailabilityService::restore_auth_then_relaunch_with(
        &old_auth(),
        &committed_auth(),
        "accounts save failed".into(),
        || Ok(false),
        |_, _| Err("restore failed".into()),
        || panic!("readback must not follow a failed restore"),
        || {
            launched.set(true);
            Ok(vec![42])
        },
    );
    assert!(!launched.get());
    assert!(error.contains("restore failed"));
}

#[test]
fn mismatched_auth_readback_never_relaunches_desktop() {
    let launched = Cell::new(false);
    let mut different = old_auth();
    different.tokens.as_mut().unwrap().refresh_token = Some("different".into());
    let error = CodexAvailabilityService::restore_auth_then_relaunch_with(
        &old_auth(),
        &committed_auth(),
        "accounts save failed".into(),
        || Ok(false),
        |_, _| Ok(()),
        || Ok(different),
        || {
            launched.set(true);
            Ok(vec![42])
        },
    );
    assert!(!launched.get());
    assert!(error.contains("readback"));
}

#[test]
fn verified_restore_relaunches_exactly_once() {
    let launched = Cell::new(0);
    let error = CodexAvailabilityService::restore_auth_then_relaunch_with(
        &old_auth(),
        &committed_auth(),
        "accounts save failed".into(),
        || Ok(false),
        |_, _| Ok(()),
        || Ok(old_auth()),
        || {
            launched.set(launched.get() + 1);
            Ok(vec![42])
        },
    );
    assert_eq!(launched.get(), 1);
    assert!(error.contains("previous account"));
}

#[test]
fn uncertain_desktop_process_state_never_restores_auth() {
    let restored = Cell::new(false);
    let error = CodexAvailabilityService::restore_auth_then_relaunch_with(
        &old_auth(),
        &committed_auth(),
        "accounts save failed".into(),
        || Err("process list unavailable".into()),
        |_, _| {
            restored.set(true);
            Ok(())
        },
        || panic!("readback must not run"),
        || panic!("relaunch must not run"),
    );
    assert!(!restored.get());
    assert!(error.contains("process inspection failed"));
}

#[test]
fn changed_auth_extension_blocks_relaunch_after_restore_readback() {
    let launched = Cell::new(false);
    let mut previous = old_auth();
    previous.extra.insert(
        "desktop_extension".into(),
        serde_json::json!({"version": 1}),
    );
    let mut changed = previous.clone();
    changed.extra.insert(
        "desktop_extension".into(),
        serde_json::json!({"version": 2}),
    );

    let error = CodexAvailabilityService::restore_auth_then_relaunch_with(
        &previous,
        &committed_auth(),
        "accounts save failed".into(),
        || Ok(false),
        |_, _| Ok(()),
        || Ok(changed),
        || {
            launched.set(true);
            Ok(vec![42])
        },
    );

    assert!(!launched.get());
    assert!(error.contains("readback"), "{error}");
}

#[test]
fn changed_auth_before_restore_is_not_overwritten_or_relaunched() {
    let env = crate::distribution::test_helper::TestEnv::new("direct_switch_restore_cas");
    let previous = old_auth();
    let mut external = old_auth();
    external.tokens.as_mut().unwrap().refresh_token = Some("synthetic-external-change".into());
    write_active_auth_json(&external).unwrap();
    let launched = Cell::new(false);

    let error = CodexAvailabilityService::restore_auth_then_relaunch_with(
        &previous,
        &committed_auth(),
        "accounts save failed".into(),
        || Ok(false),
        |expected, replacement| {
            compare_and_write_active_auth_json_for_switch(expected, replacement, || Ok(false))
        },
        read_active_auth_json,
        || {
            launched.set(true);
            Ok(vec![42])
        },
    );

    assert!(!launched.get());
    assert_eq!(read_active_auth_json().unwrap(), external);
    assert!(
        error.contains("restoration failed") || error.contains("changed"),
        "{error}"
    );
    drop(env);
}

#[test]
fn switch_auth_cas_keeps_top_level_extension_without_copying_old_token_extension() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_token_extension_boundary");
    let mut previous = old_auth();
    previous.extra.insert(
        "desktop_extension".into(),
        serde_json::json!({"version": 1}),
    );
    previous.tokens.as_mut().unwrap().extra.insert(
        "old_account_only".into(),
        serde_json::json!({"account": "old"}),
    );
    write_active_auth_json(&previous).unwrap();
    let mut replacement = committed_auth();
    replacement.extra = previous.extra.clone();

    CodexAvailabilityService::replace_auth_for_switch_with(Some(&previous), &replacement, || {
        Ok(false)
    })
    .unwrap();

    let saved = read_active_auth_json().unwrap();
    assert_eq!(saved.extra, previous.extra);
    assert_eq!(saved.tokens, replacement.tokens);
    assert!(saved
        .tokens
        .unwrap()
        .extra
        .get("old_account_only")
        .is_none());
    drop(env);
}

#[test]
fn first_auth_creation_refuses_a_new_external_auth_file() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_first_auth_create_race");
    let external = old_auth();
    let replacement = committed_auth();
    let mut probes = 0;

    let result = CodexAvailabilityService::replace_auth_for_switch_with(None, &replacement, || {
        probes += 1;
        if probes == 1 {
            let path = crate::storage::auth_json_path();
            std::fs::write(&path, serde_json::to_vec(&external).unwrap()).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        Ok(false)
    });

    assert!(result.is_err());
    assert_eq!(read_active_auth_json().unwrap(), external);
    drop(env);
}

#[test]
fn first_auth_creation_writes_private_complete_file() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_first_auth_create");
    let replacement = committed_auth();
    let path = crate::storage::auth_json_path();
    assert!(!path.exists());

    CodexAvailabilityService::replace_auth_for_switch_with(None, &replacement, || Ok(false))
        .unwrap();

    assert_eq!(read_active_auth_json().unwrap(), replacement);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(env);
}

#[test]
fn first_auth_creation_refuses_file_appearing_after_staging() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_first_auth_staged_race");
    let external = old_auth();
    let replacement = committed_auth();
    let mut probes = 0;

    let result = CodexAvailabilityService::replace_auth_for_switch_with(None, &replacement, || {
        probes += 1;
        if probes == 2 {
            let path = crate::storage::auth_json_path();
            std::fs::write(&path, serde_json::to_vec(&external).unwrap()).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        Ok(false)
    });

    assert_eq!(probes, 2);
    assert!(result.is_err());
    assert_eq!(read_active_auth_json().unwrap(), external);
    assert!(std::fs::read_dir(env.home()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("tmp.json")
    }));
    drop(env);
}

#[test]
fn first_auth_rollback_restores_absent_file_after_registry_failure() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_first_auth_rollback");
    let committed = committed_auth();
    write_active_auth_json(&committed).unwrap();

    let _ = CodexAvailabilityService::restore_auth_without_relaunch_with(
        None,
        &committed,
        "synthetic registry conflict".into(),
        || Ok(false),
    );

    assert!(!crate::storage::auth_json_path().exists());
    drop(env);
}

#[test]
fn first_auth_rollback_never_removes_external_replacement() {
    let env = crate::distribution::test_helper::TestEnv::new("switch_first_auth_rollback_external");
    let committed = committed_auth();
    let external = old_auth();
    write_active_auth_json(&external).unwrap();

    let error = CodexAvailabilityService::restore_auth_without_relaunch_with(
        None,
        &committed,
        "synthetic registry conflict".into(),
        || Ok(false),
    );

    assert!(error.contains("restoration failed"));
    assert_eq!(read_active_auth_json().unwrap(), external);
    drop(env);
}
