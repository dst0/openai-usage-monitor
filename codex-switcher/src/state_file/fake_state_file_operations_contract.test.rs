//! The fake is the only way the durability tests reach a failure, so its
//! scripting must be exact: a fault that silently stops firing, or an
//! `unfired()` that always reports nothing, would let those tests pass
//! vacuously.

use super::fake_state_file_operations::FakeStateFileOperations;
use crate::state_file::StateFileOperations;
use crate::storage::test_codex_home::TestCodexHome;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn fault_fires_only_on_its_numbered_call_and_every_call_is_logged() {
    let home = TestCodexHome::new("fake_state_ops_fault");
    let missing = home.path().join("missing");
    let files = FakeStateFileOperations::new().fail("sync_directory", 2);

    assert!(files.sync_directory(home.path()).is_ok());
    let fault = files.sync_directory(home.path()).expect_err("second call");
    assert_eq!(fault.raw_os_error(), Some(libc::EIO));
    assert!(files.sync_directory(home.path()).is_ok());
    // Unscripted calls reach the real filesystem.
    assert!(files.sync_directory(&missing).is_err());
    assert_eq!(
        files.calls(),
        vec![
            ("sync_directory", home.path().to_path_buf()),
            ("sync_directory", home.path().to_path_buf()),
            ("sync_directory", home.path().to_path_buf()),
            ("sync_directory", missing),
        ]
    );
}

#[test]
fn unfired_reports_faults_and_hooks_whose_call_never_happened() {
    let home = TestCodexHome::new("fake_state_ops_unfired");
    let ran = Rc::new(Cell::new(false));
    let seen = Rc::clone(&ran);
    let files = FakeStateFileOperations::new()
        .fail("replace", 1)
        .fail("sync_directory", 2)
        .before("sync_directory", 1, move || seen.set(true));
    assert_eq!(
        files.unfired(),
        vec![("replace", 1), ("sync_directory", 2), ("sync_directory", 1)]
    );

    assert!(files.sync_directory(home.path()).is_ok());
    assert!(ran.get(), "the hook did not run before its call");
    assert_eq!(files.unfired(), vec![("replace", 1), ("sync_directory", 2)]);
    assert!(files.sync_directory(home.path()).is_err());
    assert_eq!(files.unfired(), vec![("replace", 1)]);
}

#[test]
#[should_panic(expected = "unknown operation rename")]
fn unknown_operation_names_are_rejected() {
    let _ = FakeStateFileOperations::new().fail("rename", 1);
}
