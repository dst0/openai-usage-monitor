use super::historical_log_redaction_service::HistoricalLogRedactionService;
use crate::logger::{compress_brotli_q6, decompress_brotli};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_home(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codex_historical_redaction_{label}_{}_{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(path.join("log/archive")).unwrap();
    fs::create_dir_all(path.join("recovery-runs")).unwrap();
    fs::canonicalize(path).unwrap()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn assert_sanitized(text: &str) {
    assert!(!text.contains("legacy.person@example.test"));
    assert!(!text.contains("/private/legacy"));
    assert!(!text.contains("synthetic-secret"));
    assert!(text.contains("email_"));
    assert!(text.contains("[PATH]"));
    assert!(text.contains("[TOKEN]"));
}

fn redaction_temps(home: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for directory in [
        home.to_path_buf(),
        home.join("log"),
        home.join("log/archive"),
        home.join("recovery-runs"),
    ] {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy().starts_with(".redact-") {
                found.push(entry.path());
            }
        }
    }
    found
}

#[test]
fn sanitizes_every_owned_log_scope_preserves_foreign_files_and_is_inode_idempotent() {
    let home = temporary_home("all_scopes");
    let raw = "email=legacy.person@example.test path=/private/legacy token=synthetic-secret\n";
    for path in [
        home.join("log/switcher.log"),
        home.join("account-switcher-daemon.log"),
        home.join("account-switcher-daemon.err"),
        home.join("recovery-runs/restart-1720000000000-123.log"),
    ] {
        fs::write(path, raw).unwrap();
    }
    for prefix in [
        "switcher",
        "account-switcher-daemon",
        "account-switcher-daemon-err",
    ] {
        fs::write(
            home.join(format!("log/archive/{prefix}-20260920-010203.log.br")),
            compress_brotli_q6(raw.as_bytes()).unwrap(),
        )
        .unwrap();
    }
    let foreign = home.join("log/archive/foreign-20260920-010203.log.br");
    fs::write(&foreign, b"foreign bytes").unwrap();

    HistoricalLogRedactionService::sanitize_home(&home).unwrap();

    for path in [
        home.join("log/switcher.log"),
        home.join("account-switcher-daemon.log"),
        home.join("account-switcher-daemon.err"),
        home.join("recovery-runs/restart-1720000000000-123.log"),
    ] {
        assert_sanitized(&fs::read_to_string(&path).unwrap());
        assert_eq!(mode(&path), 0o600);
    }
    for prefix in [
        "switcher",
        "account-switcher-daemon",
        "account-switcher-daemon-err",
    ] {
        let path = home.join(format!("log/archive/{prefix}-20260920-010203.log.br"));
        let text =
            String::from_utf8(decompress_brotli(&fs::read(&path).unwrap()).unwrap()).unwrap();
        assert_sanitized(&text);
        assert_eq!(mode(&path), 0o600);
    }
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign bytes");
    let switcher = home.join("log/switcher.log");
    let before = fs::metadata(&switcher).unwrap();

    HistoricalLogRedactionService::sanitize_home(&home).unwrap();

    let after = fs::metadata(&switcher).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(before.ino(), after.ino());
    assert!(redaction_temps(&home).is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn oversized_or_invalid_input_fails_closed_without_replacing_source_or_leaving_temp_files() {
    let home = temporary_home("bounded_failure");
    let switcher = home.join("log/switcher.log");
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    fs::write(&switcher, &oversized).unwrap();
    fs::write(home.join("account-switcher-daemon.log"), b"safe\n").unwrap();
    fs::write(home.join("account-switcher-daemon.err"), b"safe\n").unwrap();

    assert!(HistoricalLogRedactionService::sanitize_home(&home).is_err());
    assert_eq!(fs::read(&switcher).unwrap(), oversized);
    assert!(redaction_temps(&home).is_empty());

    fs::write(&switcher, b"safe\n").unwrap();
    let archive = home.join("log/archive/switcher-20260920-010203.log.br");
    let oversized_archive = compress_brotli_q6(&oversized).unwrap();
    fs::write(&archive, &oversized_archive).unwrap();
    assert!(HistoricalLogRedactionService::sanitize_home(&home).is_err());
    assert_eq!(fs::read(&archive).unwrap(), oversized_archive);
    assert!(redaction_temps(&home).is_empty());

    fs::write(&switcher, [0xff, b'\n']).unwrap();
    assert!(HistoricalLogRedactionService::sanitize_home(&home).is_err());
    assert_eq!(fs::read(&switcher).unwrap(), [0xff, b'\n']);
    assert!(redaction_temps(&home).is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn malformed_brotli_and_symlinked_recovery_logs_fail_closed() {
    let home = temporary_home("malformed_archive");
    for path in [
        home.join("log/switcher.log"),
        home.join("account-switcher-daemon.log"),
        home.join("account-switcher-daemon.err"),
    ] {
        fs::write(path, b"safe\n").unwrap();
    }
    let malformed = home.join("log/archive/switcher-20260920-010203.log.br");
    fs::write(&malformed, b"not a Brotli stream").unwrap();

    assert!(HistoricalLogRedactionService::sanitize_home(&home).is_err());
    assert_eq!(fs::read(&malformed).unwrap(), b"not a Brotli stream");
    assert!(redaction_temps(&home).is_empty());
    fs::remove_dir_all(home).unwrap();

    let home = temporary_home("recovery_symlink");
    for path in [
        home.join("log/switcher.log"),
        home.join("account-switcher-daemon.log"),
        home.join("account-switcher-daemon.err"),
    ] {
        fs::write(path, b"safe\n").unwrap();
    }
    let outside = home.parent().unwrap().join(format!(
        "outside-recovery-log-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&outside, b"synthetic-secret\n").unwrap();
    symlink(
        &outside,
        home.join("recovery-runs/restart-1720000000000-123.log"),
    )
    .unwrap();

    assert!(HistoricalLogRedactionService::sanitize_home(&home).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"synthetic-secret\n");
    assert!(redaction_temps(&home).is_empty());
    fs::remove_dir_all(home).unwrap();
    fs::remove_file(outside).unwrap();
}
