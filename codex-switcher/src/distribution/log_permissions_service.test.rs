use super::LogPermissionsService;
use crate::distribution::MonitorLogCleanupService;
use crate::logger::{compress_brotli_q6, decompress_brotli};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_home(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be available")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codex_log_permissions_{label}_{}_{}",
        std::process::id(),
        nonce
    ));
    fs::create_dir_all(&path).expect("temporary home should be created");
    fs::canonicalize(path).expect("temporary home should be canonical")
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path)
        .expect("path should exist")
        .permissions()
        .mode()
        & 0o777
}

fn remove_home(path: &Path) {
    fs::remove_dir_all(path).expect("temporary home should be removable");
}

#[test]
fn rewrites_preexisting_permissive_log_paths_to_private_modes() {
    let home = temporary_home("permissive");
    let log_dir = home.join("log");
    let archive_dir = log_dir.join("archive");
    let recovery_dir = home.join("recovery-runs");
    fs::create_dir_all(&archive_dir).unwrap();
    fs::create_dir_all(&recovery_dir).unwrap();
    fs::write(log_dir.join("switcher.log"), "switcher").unwrap();
    fs::write(home.join("account-switcher-daemon.log"), "stdout").unwrap();
    fs::write(home.join("account-switcher-daemon.err"), "stderr").unwrap();
    fs::set_permissions(&log_dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&archive_dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&recovery_dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(
        log_dir.join("switcher.log"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    fs::set_permissions(
        home.join("account-switcher-daemon.log"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    fs::set_permissions(
        home.join("account-switcher-daemon.err"),
        fs::Permissions::from_mode(0o666),
    )
    .unwrap();

    LogPermissionsService::enforce_home(&home).unwrap();

    assert_eq!(mode(&log_dir), 0o700);
    assert_eq!(mode(&archive_dir), 0o700);
    assert_eq!(mode(&recovery_dir), 0o700);
    assert_eq!(mode(&log_dir.join("switcher.log")), 0o600);
    assert_eq!(mode(&home.join("account-switcher-daemon.log")), 0o600);
    assert_eq!(mode(&home.join("account-switcher-daemon.err")), 0o600);
    remove_home(&home);
}

#[test]
fn creates_missing_log_directories_without_creating_optional_files() {
    let home = temporary_home("missing");

    LogPermissionsService::enforce_home(&home).unwrap();

    assert_eq!(mode(&home.join("log")), 0o700);
    assert_eq!(mode(&home.join("log/archive")), 0o700);
    assert!(!home.join("account-switcher-daemon.log").exists());
    assert!(!home.join("account-switcher-daemon.err").exists());
    assert!(!home.join("recovery-runs").exists());
    remove_home(&home);
}

#[test]
fn rejects_symlinked_log_directory_without_following_it() {
    let home = temporary_home("symlink");
    let outside = temporary_home("symlink_target");
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o755)).unwrap();
    let log_link = home.join("log");
    symlink(&outside, &log_link).unwrap();

    let error = LogPermissionsService::enforce_home(&home).unwrap_err();

    assert_eq!(error, "unsafe log directory path");
    assert_eq!(mode(&outside), 0o755);
    assert!(fs::symlink_metadata(&log_link)
        .expect("symlink should remain")
        .file_type()
        .is_symlink());
    remove_home(&home);
    remove_home(&outside);
}

#[test]
fn rejects_symlinked_daemon_log_without_following_it() {
    let home = temporary_home("file_symlink");
    let outside = home.parent().unwrap().join(format!(
        "codex_log_permissions_file_target_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&outside, "outside").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o644)).unwrap();
    symlink(&outside, home.join("account-switcher-daemon.log")).unwrap();

    let error = LogPermissionsService::enforce_home(&home).unwrap_err();

    assert_eq!(error, "unsafe daemon log path");
    assert_eq!(mode(&outside), 0o644);
    fs::remove_file(&outside).unwrap();
    remove_home(&home);
}

#[test]
fn rejects_symlinked_switcher_log_without_following_it() {
    let home = temporary_home("switcher_symlink");
    let log_dir = home.join("log");
    fs::create_dir_all(log_dir.join("archive")).unwrap();
    let outside = home.parent().unwrap().join(format!(
        "codex_log_permissions_switcher_target_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&outside, "outside").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o644)).unwrap();
    symlink(&outside, log_dir.join("switcher.log")).unwrap();

    let error = LogPermissionsService::enforce_home(&home).unwrap_err();

    assert_eq!(error, "unsafe switcher log path");
    assert_eq!(mode(&outside), 0o644);
    fs::remove_file(&outside).unwrap();
    remove_home(&home);
}

#[test]
fn install_redacts_preexisting_active_logs_and_brotli_archives_idempotently() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = temporary_home("historical_redaction");
    let previous_home = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &home);

    let log_dir = home.join("log");
    let archive_dir = log_dir.join("archive");
    fs::create_dir_all(&archive_dir).unwrap();
    let raw = "email=legacy.person@example.test path=/private/legacy token=synthetic-secret\n";
    fs::write(log_dir.join("switcher.log"), raw).unwrap();
    fs::write(home.join("account-switcher-daemon.log"), raw).unwrap();
    fs::write(home.join("account-switcher-daemon.err"), raw).unwrap();
    let archive_path = archive_dir.join("switcher-20260920-010203.log.br");
    fs::write(&archive_path, compress_brotli_q6(raw.as_bytes()).unwrap()).unwrap();

    MonitorLogCleanupService::install().unwrap();

    let active = fs::read_to_string(log_dir.join("switcher.log")).unwrap();
    let archive =
        String::from_utf8(decompress_brotli(&fs::read(&archive_path).unwrap()).unwrap()).unwrap();
    for sanitized in [&active, &archive] {
        assert!(!sanitized.contains("legacy.person@example.test"));
        assert!(!sanitized.contains("/private/legacy"));
        assert!(!sanitized.contains("synthetic-secret"));
        assert!(sanitized.contains("email_"));
        assert!(sanitized.contains("[PATH]"));
        assert!(sanitized.contains("[TOKEN]"));
    }

    MonitorLogCleanupService::install().unwrap();
    assert_eq!(
        active,
        fs::read_to_string(log_dir.join("switcher.log")).unwrap()
    );
    assert_eq!(
        archive,
        String::from_utf8(decompress_brotli(&fs::read(&archive_path).unwrap()).unwrap()).unwrap()
    );

    match previous_home {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    remove_home(&home);
}
