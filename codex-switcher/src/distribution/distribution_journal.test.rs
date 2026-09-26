use super::DistributionJournal;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;

fn home(label: &str) -> PathBuf {
    let mut nonce = [0u8; 8];
    getrandom::getrandom(&mut nonce).unwrap();
    let path = std::env::temp_dir().join(format!(
        "distribution_journal_{label}_{}_{:016x}",
        std::process::id(),
        u64::from_ne_bytes(nonce)
    ));
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn journal() -> DistributionJournal {
    let now = chrono::Utc::now().to_rfc3339();
    DistributionJournal {
        operation_id: "synthetic-operation".into(),
        pid: std::process::id(),
        trigger: "user".into(),
        reason: "synthetic".into(),
        target_app_id: None,
        target_cli_id: None,
        phase: "initialized".into(),
        started_at: now.clone(),
        updated_at: now,
    }
}

#[test]
fn save_never_follows_predictable_pid_staging_symlink() {
    let home = home("staging_symlink");
    let victim = home.join("synthetic-victim");
    fs::write(&victim, b"keep synthetic data").unwrap();
    let predictable = home.join(format!("distribution-journal.{}.tmp", std::process::id()));
    symlink(&victim, &predictable).unwrap();

    journal().save(&home).unwrap();

    assert!(fs::read(&victim).unwrap() == b"keep synthetic data");
    assert!(fs::symlink_metadata(&predictable)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        DistributionJournal::load(&home)
            .unwrap()
            .unwrap()
            .operation_id,
        "synthetic-operation"
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn load_rejects_symlink_even_with_valid_json_target() {
    let home = home("load_symlink");
    let target = home.join("synthetic-valid-journal");
    fs::write(&target, serde_json::to_vec(&journal()).unwrap()).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, DistributionJournal::journal_path(&home)).unwrap();

    assert!(DistributionJournal::load(&home).is_err());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn load_rejects_valid_oversized_document_and_broad_mode() {
    let home = home("bounded_load");
    let path = DistributionJournal::journal_path(&home);
    let mut oversized = journal();
    oversized.reason = "x".repeat(128 * 1024);
    fs::write(&path, serde_json::to_vec(&oversized).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(DistributionJournal::load(&home).is_err());

    fs::write(&path, serde_json::to_vec(&journal()).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(DistributionJournal::load(&home).is_err());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn save_refuses_unsafe_final_symlink_without_touching_target() {
    let home = home("final_symlink");
    let victim = home.join("synthetic-victim");
    fs::write(&victim, b"keep synthetic data").unwrap();
    let path = DistributionJournal::journal_path(&home);
    symlink(&victim, &path).unwrap();

    assert!(journal().save(&home).is_err());
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(&victim).unwrap(), b"keep synthetic data");
    assert!(!fs::read_dir(&home).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn save_refuses_broad_mode_final_without_replacing_it() {
    let home = home("broad_final");
    let path = DistributionJournal::journal_path(&home);
    let old = serde_json::to_vec(&journal()).unwrap();
    fs::write(&path, &old).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    assert!(journal().save(&home).is_err());
    assert_eq!(fs::read(&path).unwrap(), old);
    fs::remove_dir_all(home).unwrap();
}
