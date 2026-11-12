use super::DeferredManifestFilter;
use crate::recovery::manifest_store::{load_manifest, write_manifest};
use crate::recovery::pending_target::PendingTarget;
use crate::storage::test_codex_home::TestCodexHome;

#[test]
fn empty_or_completed_manifest_returns_empty_without_error() {
    let home = TestCodexHome::new("deferred-manifest-empty");
    write_manifest(&[]).unwrap();
    let loaded = DeferredManifestFilter::prune_and_load_active(&home.path()).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn ownerless_target_missing_from_sqlite_is_pruned_from_disk() {
    let home = TestCodexHome::new("deferred-manifest-missing-sqlite");
    let stale = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e79".into(),
        offset: Some(10),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
        auth_rotation: None,
    };
    write_manifest(&[stale]).unwrap();
    let loaded = DeferredManifestFilter::prune_and_load_active(&home.path()).unwrap();
    assert!(loaded.is_empty());
    let reloaded = load_manifest().unwrap();
    assert!(reloaded.is_empty());
}
