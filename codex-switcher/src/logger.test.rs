use super::*;
use crate::distribution::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use crate::distribution::{DistributionAuditLogger, LogRedactionService};
use std::fs;
use std::os::unix::fs::symlink;

fn temporary_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("codex_logger_{label}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    let path = fs::canonicalize(path).unwrap();
    MonitorLogLifecycleLock::ensure(&path).unwrap();
    path
}

#[test]
fn test_brotli_q6_compression_and_decompression_roundtrip() {
    let original_data = b"2026-09-14T21:38:00Z [INFO] [USER_SWITCH] Switching account\n\
                          2026-09-14T21:38:05Z [INFO] [RECOVERY] RECOVERY_STARTED\n\
                          2026-09-14T21:38:15Z [INFO] [RECOVERY] RECOVERY_VERIFIED\n"
        .repeat(20);
    let compressed = compress_brotli_q6(&original_data).expect("Compression should succeed");
    assert!(compressed.len() < original_data.len());
    assert_eq!(decompress_brotli(&compressed).unwrap(), original_data);
}

#[test]
fn test_rotate_file_if_needed_compresses_and_truncates() {
    let temp_dir = temporary_dir("rotate");
    let log_file = temp_dir.join("test_switcher.log");
    let test_data = "Log line entry for test rotation\n".repeat(100);
    fs::write(&log_file, &test_data).unwrap();
    let initial_size = fs::metadata(&log_file).unwrap().len();

    let result = rotate_file_if_needed(&log_file, "test-prefix", 500, 5)
        .unwrap()
        .unwrap();
    assert_eq!(result.original_bytes, initial_size);
    assert!(result.compressed_bytes < initial_size);
    assert_eq!(fs::metadata(&log_file).unwrap().len(), 0);
    assert!(result
        .archive_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with(".log.br"));
    assert_eq!(fs::read_dir(temp_dir.join("archive")).unwrap().count(), 1);
}

#[test]
fn test_prune_archives_bounds_retention_and_rejects_malformed_timestamp() {
    let temp_dir = temporary_dir("prune");
    let archive_dir = temp_dir.join("archive");
    fs::create_dir_all(&archive_dir).unwrap();
    for i in 1..=10 {
        fs::write(
            archive_dir.join(format!("test-prefix-20260914-000{i:03}.log.br")),
            b"test",
        )
        .unwrap();
    }
    fs::write(
        archive_dir.join("test-prefix-20260914-0000xx.log.br"),
        b"keep",
    )
    .unwrap();
    let directory = MonitorLogIoService::open_directory_path(&archive_dir, false).unwrap();
    prune_archives(&directory, "test-prefix", 5);

    let names: Vec<_> = fs::read_dir(&archive_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with("test-prefix-"))
            .count(),
        6
    );
    assert!(archive_dir
        .join("test-prefix-20260914-0000xx.log.br")
        .exists());
}

#[test]
fn test_logger_write_rejects_symlinked_active_file() {
    let temp_dir = temporary_dir("write_symlink");
    let target = temp_dir.join("external.log");
    fs::write(&target, b"external").unwrap();
    let link = temp_dir.join("switcher.log");
    symlink(&target, &link).unwrap();

    let result = MonitorLogIoService::append(&link, b"must not append\n");
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "external");
}

#[test]
fn test_logger_rotation_rejects_symlinked_active_or_archive_parent() {
    let temp_dir = temporary_dir("rotation_symlink");
    let external = temporary_dir("rotation_external");
    let target = external.join("active.log");
    fs::write(&target, "external\n".repeat(20)).unwrap();
    let active_link = temp_dir.join("active.log");
    symlink(&target, &active_link).unwrap();
    assert!(rotate_file_if_needed(&active_link, "switcher", 1, 5).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "external\n".repeat(20));

    let archive_target = temporary_dir("archive_external");
    let active = temp_dir.join("safe.log");
    fs::write(&active, "safe\n".repeat(20)).unwrap();
    symlink(&archive_target, temp_dir.join("archive")).unwrap();
    assert!(rotate_file_if_needed(&active, "switcher", 1, 5).is_err());
    assert!(fs::read_dir(archive_target).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("switcher-")
    }));
}

#[test]
fn logger_and_audit_share_redaction_before_brotli_rotation() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_dir = temporary_dir("redaction");
    let previous_home = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &temp_dir);
    let raw = "email=quoted.user@example.test thread=550e8400-e29b-41d4-a716-446655440000 path=/Users/dst/private Bearer secret";
    log("INFO", "RECOVERY", raw);
    DistributionAuditLogger::new(switcher_log_path()).log_action(
        "op_test",
        "RECOVERY",
        "auto",
        "quota_exhausted",
        raw,
    );

    let content = fs::read_to_string(switcher_log_path()).unwrap();
    let clean = LogRedactionService::sanitize_text(raw);
    assert!(content.contains(&clean));
    assert!(!content.contains("quoted.user@example.test"));
    assert!(!content.contains("550e8400-e29b-41d4-a716-446655440000"));
    assert!(!content.contains("/Users/dst/private"));
    assert!(!content.contains("secret"));

    let compressed = compress_brotli_q6(content.as_bytes()).unwrap();
    let roundtrip = String::from_utf8(decompress_brotli(&compressed).unwrap()).unwrap();
    assert!(!roundtrip.contains("quoted.user@example.test"));
    assert!(roundtrip.contains("[PATH]"));

    match previous_home {
        Some(home) => std::env::set_var("CODEX_HOME", home),
        None => std::env::remove_var("CODEX_HOME"),
    }
    let _ = fs::remove_dir_all(temp_dir);
}
