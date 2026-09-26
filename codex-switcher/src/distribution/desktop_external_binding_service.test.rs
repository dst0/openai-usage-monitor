use super::DesktopExternalBindingService;
use crate::distribution::test_helper::make_account;
use crate::distribution::{DesktopAppSession, WindowProcessIdentity};
use crate::models::{AccountsFile, AuthJson, Settings};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn moment(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn old_session() -> DesktopAppSession {
    let mut session = DesktopAppSession::bound(
        "account-a",
        "account-a",
        WindowProcessIdentity::new(100, "1000:000000").unwrap(),
    );
    session.updated_at = "1970-01-01T00:16:41Z".into();
    session
}

fn private_json(path: &Path, value: &impl serde::Serialize) {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
    file.sync_all().unwrap();
}

#[path = "desktop_external_binding_candidate.test.rs"]
mod candidate_tests;

#[path = "desktop_external_binding_runtime.test.rs"]
mod runtime_tests;

#[path = "desktop_external_binding_security.test.rs"]
mod security_tests;
