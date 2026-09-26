use super::{
    manifest_store::{load_manifest, write_manifest},
    pending_target::PendingTarget,
    recovery_manifest_snapshot::RecoveryManifestSnapshot,
};

#[test]
fn rejected_shutdown_restores_previous_ownerless_retry() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("window_recheck_rollback");
    let old = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e79".into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    write_manifest(std::slice::from_ref(&old)).unwrap();
    let snapshot = RecoveryManifestSnapshot::capture().unwrap();
    write_manifest(&[PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
        offset: Some(99),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    }])
    .unwrap();
    assert_eq!(
        snapshot.rollback_error("window count changed".into()),
        "window count changed"
    );
    let restored = load_manifest().unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, old.id);
    assert_eq!(restored[0].offset, old.offset);
    assert_eq!(restored[0].owner_account_id, old.owner_account_id);
    assert!(env.home().join("desktop-recovery.json").exists());
}

#[test]
fn failed_journal_rollback_is_reported_with_original_shutdown_error() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("window_rollback_error");
    let snapshot = RecoveryManifestSnapshot::capture().unwrap();
    std::fs::create_dir(env.home().join("desktop-recovery.json")).unwrap();
    let error = snapshot.rollback_error("window inventory changed".into());
    assert!(error.contains("window inventory changed"));
    assert!(error.contains("prior recovery journal could not be restored"));
}
