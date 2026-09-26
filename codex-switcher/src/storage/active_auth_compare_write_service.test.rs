use super::ActiveAuthCompareWriteService;
use crate::models::{AuthJson, AuthTokens};
use std::cell::Cell;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

fn auth(refresh: &str) -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: "fixture-access".into(),
            refresh_token: Some(refresh.into()),
            id_token: None,
            account_id: Some("fixture-workspace".into()),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    }
}

fn fixture() -> (PathBuf, AuthJson) {
    let directory = std::env::temp_dir().join(format!(
        "codex-auth-cas-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("auth.json");
    let original = auth("original-refresh");
    fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    (path, original)
}

fn read_auth(path: &Path) -> AuthJson {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn cleanup(path: &Path) {
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn compare_write_replaces_exact_expected_file_with_private_mode() {
    let (path, original) = fixture();
    let replacement = auth("new-refresh");
    let probes = Cell::new(0);
    ActiveAuthCompareWriteService::new(&path)
        .execute(&original, &replacement, || {
            probes.set(probes.get() + 1);
            Ok(false)
        })
        .unwrap();
    assert!(probes.get() >= 3);
    assert_eq!(read_auth(&path).tokens, replacement.tokens);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    cleanup(&path);
}

#[test]
fn compare_write_preserves_unknown_auth_and_token_fields() {
    let (path, _) = fixture();
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["desktop_only"] = serde_json::json!({"state": 7});
    raw["tokens"]["issuer_extension"] = serde_json::json!({"version": 3});
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let original = read_auth(&path);
    let replacement = auth("new-refresh");
    ActiveAuthCompareWriteService::new(&path)
        .execute(&original, &replacement, || Ok(false))
        .unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["desktop_only"], raw["desktop_only"]);
    assert_eq!(
        saved["tokens"]["issuer_extension"],
        raw["tokens"]["issuer_extension"]
    );
    assert_eq!(saved["tokens"]["refresh_token"], "new-refresh");
    cleanup(&path);
}

#[test]
fn compare_write_rejects_unknown_auth_field_change_after_staging() {
    let (path, _) = fixture();
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["desktop_only"] = serde_json::json!({"state": 7});
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let expected = read_auth(&path);
    let result = ActiveAuthCompareWriteService::new(&path).execute_with_hook(
        &expected,
        &auth("new-refresh"),
        || Ok(false),
        || {
            let mut changed: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            changed["desktop_only"]["state"] = serde_json::json!(8);
            fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        },
    );
    assert!(result
        .unwrap_err()
        .contains("changed before re-login commit"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["desktop_only"]["state"], 8);
    assert_eq!(saved["tokens"]["refresh_token"], "original-refresh");
    cleanup(&path);
}

#[test]
fn compare_write_detects_in_place_rotation_after_staging() {
    let (path, original) = fixture();
    let external = auth("external-refresh");
    let result = ActiveAuthCompareWriteService::new(&path).execute_with_hook(
        &original,
        &auth("new-refresh"),
        || Ok(false),
        || fs::write(&path, serde_json::to_vec(&external).unwrap()).unwrap(),
    );
    assert!(result
        .unwrap_err()
        .contains("changed before re-login commit"));
    assert_eq!(read_auth(&path).tokens, external.tokens);
    cleanup(&path);
}

#[test]
fn compare_write_detects_path_replacement_after_staging() {
    let (path, original) = fixture();
    let external = auth("external-refresh");
    let result = ActiveAuthCompareWriteService::new(&path).execute_with_hook(
        &original,
        &auth("new-refresh"),
        || Ok(false),
        || {
            let other = path.with_extension("external");
            fs::write(&other, serde_json::to_vec(&external).unwrap()).unwrap();
            fs::rename(other, &path).unwrap();
        },
    );
    assert!(result.unwrap_err().contains("identity changed"));
    assert_eq!(read_auth(&path).tokens, external.tokens);
    cleanup(&path);
}

#[test]
fn compare_write_stops_when_desktop_writer_starts_before_rename() {
    let (path, original) = fixture();
    let active = Cell::new(false);
    let result = ActiveAuthCompareWriteService::new(&path).execute_with_hook(
        &original,
        &auth("new-refresh"),
        || Ok(active.get()),
        || active.set(true),
    );
    assert!(result.unwrap_err().contains("became active"));
    assert_eq!(read_auth(&path).tokens, original.tokens);
    cleanup(&path);
}

#[test]
fn compare_write_stops_when_process_probe_fails_before_rename() {
    let (path, original) = fixture();
    let probes = Cell::new(0);
    let result =
        ActiveAuthCompareWriteService::new(&path).execute(&original, &auth("new-refresh"), || {
            probes.set(probes.get() + 1);
            if probes.get() == 2 {
                Err("Desktop process inspection failed".into())
            } else {
                Ok(false)
            }
        });
    assert!(result.unwrap_err().contains("inspection failed"));
    assert_eq!(read_auth(&path).tokens, original.tokens);
    cleanup(&path);
}
