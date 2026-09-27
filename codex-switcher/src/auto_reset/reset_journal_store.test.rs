use super::ResetJournalStore;
use crate::auto_reset::reset_journal::ResetJournal;
use crate::state_file::fake_state_file_operations::FakeStateFileOperations;
use crate::storage::test_codex_home::TestCodexHome;

fn pending() -> ResetJournal {
    ResetJournal {
        episode_key: Some("account|window".into()),
        account_id: Some("account".into()),
        thread_id: Some("synthetic-task".into()),
        idempotency_key: Some("00000000-0000-4000-8000-000000000001".into()),
        state: "pending".into(),
        updated_at: Some("2026-01-01T00:00:00Z".into()),
        ..ResetJournal::default()
    }
}

/// A `pending` marker precedes a credit request. The rename that publishes it
/// must be followed by a directory flush, or power loss can resurrect the
/// previous journal after the request left.
#[test]
fn journal_write_flushes_the_directory_after_the_rename() {
    let home = TestCodexHome::new("auto_journal_durability");
    let files = FakeStateFileOperations::new();
    let store = ResetJournalStore::in_directory(home.path().to_path_buf(), &files);
    let written = store.write(&pending());
    let calls = files.calls();

    assert!(written.is_ok(), "{written:?}");
    let rename = calls
        .iter()
        .position(|(operation, _)| *operation == "replace")
        .expect("the journal was renamed into place");
    assert_eq!(
        calls[rename..].to_vec(),
        vec![
            ("replace", home.path().join("auto-reset-state.json")),
            ("sync_directory", home.path().to_path_buf()),
        ]
    );
    assert_eq!(store.load().unwrap().state, "pending");
}

#[test]
fn journal_directory_sync_failure_is_reported_not_swallowed() {
    let home = TestCodexHome::new("auto_journal_sync_failure");
    let files = FakeStateFileOperations::new().fail("sync_directory", 1);
    let store = ResetJournalStore::in_directory(home.path().to_path_buf(), &files);
    let written = store.write(&pending());

    let error = written.expect_err("an unflushed rename must not report success");
    assert!(error.contains("could not be synced"), "{error}");
    assert!(files.unfired().is_empty());
}
