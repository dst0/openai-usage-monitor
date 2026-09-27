use super::PrivateStateFileWriteService;
use crate::state_file::fake_state_file_operations::FakeStateFileOperations;
use crate::state_file::StateFileWriteFailure;
use crate::storage::test_codex_home::TestCodexHome;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const FILE: &str = "synthetic-state.json";
const STAGING: &str = ".synthetic-state.1.1.tmp";

fn replace(
    operations: &FakeStateFileOperations,
    directory: &Path,
    content: &[u8],
) -> Result<(), StateFileWriteFailure> {
    PrivateStateFileWriteService::new(operations).replace(directory, FILE, STAGING, content)
}

fn stage(failure: &StateFileWriteFailure) -> &'static str {
    match failure {
        StateFileWriteFailure::PrepareDirectory(_) => "prepare_directory",
        StateFileWriteFailure::CreateStaging(_) => "create_staging",
        StateFileWriteFailure::SaveStaging(_) => "save_staging",
        StateFileWriteFailure::Replace(_) => "replace",
        StateFileWriteFailure::SyncDirectory(_) => "sync_directory",
    }
}

#[test]
fn replace_flushes_the_directory_after_the_rename() {
    let home = TestCodexHome::new("state_file_order");
    let directory = home.path().join("state");
    let operations = FakeStateFileOperations::new();
    let result = replace(&operations, &directory, b"new");

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        operations.calls(),
        vec![
            ("prepare_directory", directory.clone()),
            ("create_staging", directory.join(STAGING)),
            ("save_staging", PathBuf::new()),
            ("replace", directory.join(FILE)),
            ("sync_directory", directory.clone()),
        ]
    );
    let path = directory.join(FILE);
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&path), mode(&directory)), (0o600, 0o700));
    assert!(
        !directory.join(STAGING).exists(),
        "staging file left behind"
    );
}

/// Up to and including the rename, a failure leaves the previous document and
/// removes only a staging file this write created.
#[test]
fn failure_before_the_rename_keeps_the_previous_document() {
    let cases = [
        ("prepare_directory", false),
        ("create_staging", false),
        ("save_staging", true),
        ("replace", true),
    ];
    for (failing, removes_staging) in cases {
        let home = TestCodexHome::new("state_file_before_rename");
        let directory = home.path().to_path_buf();
        replace(&FakeStateFileOperations::new(), &directory, b"old").unwrap();
        let operations = FakeStateFileOperations::new().fail(failing, 1);
        let result = replace(&operations, &directory, b"new");

        let failure = result.expect_err(failing);
        assert_eq!(stage(&failure), failing);
        assert_eq!(
            std::fs::read(directory.join(FILE)).unwrap(),
            b"old",
            "{failing}"
        );
        assert!(!directory.join(STAGING).exists(), "{failing}: staging kept");
        assert_eq!(
            operations.operations().contains(&"remove_staging"),
            removes_staging,
            "{failing}: staging cleanup"
        );
        assert!(
            !operations.operations().contains(&"sync_directory"),
            "{failing}: synced after a failed rename"
        );
        assert!(operations.unfired().is_empty(), "{failing}");
    }
}

#[test]
fn directory_sync_failure_leaves_the_new_content_visible() {
    let home = TestCodexHome::new("state_file_sync");
    let directory = home.path().to_path_buf();
    replace(&FakeStateFileOperations::new(), &directory, b"old").unwrap();
    let operations = FakeStateFileOperations::new().fail("sync_directory", 1);
    let failure = replace(&operations, &directory, b"new").expect_err("sync failure");

    assert_eq!(stage(&failure), "sync_directory");
    assert!(
        failure
            .to_string()
            .starts_with("state directory could not be synced: "),
        "{failure}"
    );
    assert_eq!(std::fs::read(directory.join(FILE)).unwrap(), b"new");
    assert!(!directory.join(STAGING).exists());
    assert!(!operations.operations().contains(&"remove_staging"));
}

/// Another file at the staging name, including a symlink, is neither followed
/// nor removed.
#[test]
fn existing_staging_name_is_never_followed_or_removed() {
    let home = TestCodexHome::new("state_file_staging_link");
    let directory = home.path().to_path_buf();
    let victim = directory.join("synthetic-victim.txt");
    std::fs::write(&victim, b"keep").unwrap();
    std::os::unix::fs::symlink(&victim, directory.join(STAGING)).unwrap();
    let operations = FakeStateFileOperations::new();
    let failure = replace(&operations, &directory, b"new").expect_err("existing staging");

    assert_eq!(stage(&failure), "create_staging");
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    assert!(std::fs::symlink_metadata(directory.join(STAGING))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!directory.join(FILE).exists());
    assert!(!operations.operations().contains(&"remove_staging"));
}
