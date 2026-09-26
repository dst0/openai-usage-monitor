use super::DesktopAppSession;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn session_save_does_not_follow_predictable_temporary_symlink() {
    let home = std::env::temp_dir().join(format!(
        "desktop-session-temp-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let marker = home.join("desktop-app-session.json");
    let old_temp = marker.with_extension(format!("{}.tmp", std::process::id()));
    let victim = home.join("synthetic-victim.txt");
    std::fs::write(&victim, b"keep synthetic data").unwrap();
    symlink(&victim, &old_temp).unwrap();

    let result = DesktopAppSession::new("test-account").save(&marker);
    assert!(result.is_ok());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep synthetic data");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn session_load_rejects_symlink_and_oversized_private_marker() {
    let home = std::env::temp_dir().join(format!(
        "desktop-session-load-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let marker = home.join("desktop-app-session.json");
    let target = home.join("other.json");
    std::fs::write(&target, b"{}").unwrap();
    symlink(&target, &marker).unwrap();
    assert!(DesktopAppSession::load_checked(&marker).is_err());
    assert!(DesktopAppSession::new("test").save(&marker).is_err());
    std::fs::remove_file(&marker).unwrap();

    std::fs::write(&marker, vec![b' '; 16 * 1024 + 1]).unwrap();
    std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(DesktopAppSession::load_checked(&marker).is_err());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn post_rename_failure_restores_previous_marker() {
    let home = std::env::temp_dir().join(format!(
        "desktop-session-rollback-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let path = home.join("desktop-app-session.json");
    let previous = DesktopAppSession::new("previous");
    let attempted = DesktopAppSession::new("attempted");
    previous.save(&path).unwrap();
    assert!(attempted
        .save_with_post_rename(&path, |_| Err("synthetic post-rename failure".into()))
        .is_err());
    assert_eq!(
        DesktopAppSession::load_checked(&path).unwrap(),
        Some(attempted.clone())
    );
    DesktopAppSession::restore_after_failed_save(&path, Some(&previous), &attempted).unwrap();
    assert_eq!(
        DesktopAppSession::load_checked(&path).unwrap(),
        Some(previous)
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn post_rename_failure_does_not_overwrite_a_newer_marker() {
    let home = std::env::temp_dir().join(format!(
        "desktop-session-rollback-race-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let path = home.join("desktop-app-session.json");
    let previous = DesktopAppSession::new("previous");
    let attempted = DesktopAppSession::new("attempted");
    let newer = DesktopAppSession::new("newer");
    previous.save(&path).unwrap();
    assert!(attempted
        .save_with_post_rename(&path, |_| Err("synthetic post-rename failure".into()))
        .is_err());
    newer.save(&path).unwrap();
    assert!(
        DesktopAppSession::restore_after_failed_save(&path, Some(&previous), &attempted).is_err()
    );
    assert_eq!(DesktopAppSession::load_checked(&path).unwrap(), Some(newer));
    std::fs::remove_dir_all(home).unwrap();
}
