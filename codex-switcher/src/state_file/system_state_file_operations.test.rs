use super::SystemStateFileOperations;
use crate::state_file::StateFileOperations;
use crate::storage::test_codex_home::TestCodexHome;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn prepare_directory_creates_and_restricts_the_directory() {
    let home = TestCodexHome::new("state_ops_directory");
    let created = home.path().join("new/nested");
    let existing = home.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();

    SystemStateFileOperations
        .prepare_directory(&created)
        .unwrap();
    SystemStateFileOperations
        .prepare_directory(&existing)
        .unwrap();
    assert_eq!((mode(&created), mode(&existing)), (0o700, 0o700));
}

/// The flush itself cannot be observed without power loss; this proves the
/// call opens and syncs the named directory rather than doing nothing.
#[test]
fn sync_directory_opens_the_named_directory() {
    let home = TestCodexHome::new("state_ops_sync");
    assert!(SystemStateFileOperations
        .sync_directory(home.path())
        .is_ok());
    let missing = SystemStateFileOperations
        .sync_directory(&home.path().join("missing"))
        .expect_err("a missing directory cannot be synced");
    assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
}

/// Readers require exactly 0600, and the creation mode is filtered by the
/// umask, so saving must set the mode on the descriptor itself.
#[test]
fn save_staging_writes_the_content_and_forces_owner_only_mode() {
    let home = TestCodexHome::new("state_ops_save");
    let staging = home.path().join("staging.tmp");
    fs::write(&staging, b"").unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o644)).unwrap();
    let mut file = OpenOptions::new().write(true).open(&staging).unwrap();

    SystemStateFileOperations
        .save_staging(&mut file, b"content")
        .unwrap();
    assert_eq!(mode(&staging), 0o600);
    assert_eq!(fs::read(&staging).unwrap(), b"content");
}

#[test]
fn open_for_read_never_follows_a_symlink() {
    let home = TestCodexHome::new("state_ops_open");
    let target = home.path().join("state.json");
    fs::write(&target, b"private").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let link = home.path().join("link.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let mut content = String::new();
    SystemStateFileOperations
        .open_for_read(&target)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(content, "private");
    assert!(
        SystemStateFileOperations.open_for_read(&link).is_err(),
        "a symlink to a valid private file was followed"
    );
}

#[test]
fn staging_is_exclusive_and_owner_only() {
    let home = TestCodexHome::new("state_ops_staging");
    let staging = home.path().join("staging.tmp");
    drop(SystemStateFileOperations.create_staging(&staging).unwrap());
    assert_eq!(mode(&staging), 0o600);
    let reused = SystemStateFileOperations
        .create_staging(&staging)
        .expect_err("an existing staging name was reused");
    assert_eq!(reused.kind(), std::io::ErrorKind::AlreadyExists);
    SystemStateFileOperations.remove_staging(&staging).unwrap();
    assert!(!staging.exists());
}
