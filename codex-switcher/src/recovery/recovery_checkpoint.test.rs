use super::{
    dispatch_mark_error::DispatchMarkError,
    manifest_store::{
        finalize_target, load_manifest, load_ownerless_pending, mark_dispatch_attempt_for_account,
        write_manifest,
    },
    pending_target::PendingTarget,
    recovery_checkpoint::checkpoint_targets_with,
    recovery_mode::RecoveryMode,
};

const STALE: &str = "01a0d9eb-87c8-7213-b62d-a312d00c4ae5";
const OTHER: &str = "01a0d9ec-7059-7432-93ae-866965f28ac3";
const OWNER: &str = "owner@example.test:3f533057-0000-0000-0000-000000000000";
const SECOND: &str = "second@example.test:aef6d346-0000-0000-0000-000000000000";
const STALE_OFFSET: u64 = 134_933_394;
const FRESH_OFFSET: u64 = 135_000_000;

/// The journal entry left by a captured restart whose task never mounted.
fn deferred(id: &str, owner: &str) -> PendingTarget {
    PendingTarget {
        id: id.into(),
        offset: Some(STALE_OFFSET),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some(owner.into()),
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn checkpoint(
    manifest: &mut Vec<PendingTarget>,
    targets: &[&str],
    mode: RecoveryMode,
    binding: Option<&str>,
) -> Result<(Vec<PendingTarget>, Vec<String>), String> {
    checkpoint_targets_with(manifest, &ids(targets), mode, binding, |_| {
        Some(FRESH_OFFSET)
    })
}

fn assert_original_deferred(target: &PendingTarget) {
    assert!(target.awaiting_owner);
    assert!(target.captured_restart);
    assert_eq!(target.owner_account_id.as_deref(), Some(OWNER));
    assert_eq!(target.offset, Some(STALE_OFFSET));
}

#[test]
fn explicit_resume_claims_stale_deferred_target_only_in_memory() {
    for binding in [Some(OWNER), Some(SECOND), None] {
        let mut manifest = vec![deferred(STALE, OWNER)];
        let (view, claimed) = checkpoint(
            &mut manifest,
            &[STALE],
            RecoveryMode::ExplicitTarget,
            binding,
        )
        .unwrap_or_else(|error| panic!("binding {binding:?} blocked explicit resume: {error}"));
        assert_eq!(claimed, ids(&[STALE]));
        // This operation observes from a fresh checkpoint without a binding...
        assert_eq!(view.len(), 1);
        assert!(!view[0].awaiting_owner);
        assert_eq!(view[0].owner_account_id, None);
        assert_eq!(view[0].offset, Some(FRESH_OFFSET));
        // ...while the journal keeps the deferred retry exactly as it was.
        assert_eq!(manifest.len(), 1);
        assert_original_deferred(&manifest[0]);
    }
}

#[test]
fn stale_deferred_binding_no_longer_blocks_the_explicit_dispatch_marker() {
    let _home = crate::storage::test_codex_home::TestCodexHome::new("checkpoint-marker");

    // Regression for `RECOVERY_FAILED reason=Active Desktop account changed
    // before deferred recovery`: the pre-IPC marker compared the deferred
    // binding with a re-read account binding even for an explicit request.
    write_manifest(&[deferred(STALE, OWNER), deferred(OTHER, OWNER)]).unwrap();
    for binding in [None, Some(SECOND)] {
        assert!(matches!(
            mark_dispatch_attempt_for_account(STALE, binding, RecoveryMode::DeferredCaptured),
            Err(DispatchMarkError::AccountChanged)
        ));
    }

    let mut manifest = load_manifest().unwrap();
    checkpoint(
        &mut manifest,
        &[STALE],
        RecoveryMode::ExplicitTarget,
        Some(OWNER),
    )
    .unwrap();
    write_manifest(&manifest).unwrap();
    // A crash between checkpoint and dispatch leaves the task deferred and
    // bound: discovery keeps excluding it as ownerless, so no later unattended
    // restart may pick it up as ordinary pending work under another account.
    assert_original_deferred(&load_manifest().unwrap()[0]);
    assert!(load_ownerless_pending()
        .unwrap()
        .contains(&STALE.to_string()));

    mark_dispatch_attempt_for_account(STALE, None, RecoveryMode::ExplicitTarget).unwrap();
    let remaining = load_manifest().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, OTHER);
    assert!(remaining[0].awaiting_owner);
}

#[test]
fn automatic_modes_still_honor_the_deferred_account_binding() {
    for mode in [
        RecoveryMode::CapturedRestart,
        RecoveryMode::DeferredCaptured,
        RecoveryMode::DeferredOwned,
        RecoveryMode::DiscoveredOnly,
    ] {
        for binding in [Some(SECOND), None] {
            let mut manifest = vec![deferred(STALE, OWNER)];
            let error = checkpoint(&mut manifest, &[STALE], mode, binding).unwrap_err();
            assert_eq!(
                error,
                "Deferred recovery belongs to a different Desktop account"
            );
            assert_original_deferred(&manifest[0]);
        }
        // The matching account keeps the original pre-restart checkpoint.
        let mut manifest = vec![deferred(STALE, OWNER)];
        let (view, claimed) = checkpoint(&mut manifest, &[STALE], mode, Some(OWNER)).unwrap();
        assert!(claimed.is_empty());
        assert_original_deferred(&view[0]);
        assert_original_deferred(&manifest[0]);
    }
}

#[test]
fn explicit_claim_leaves_other_deferred_targets_bound() {
    let mut manifest = vec![deferred(STALE, OWNER), deferred(OTHER, SECOND)];
    let (view, claimed) = checkpoint(
        &mut manifest,
        &[STALE],
        RecoveryMode::ExplicitTarget,
        Some(OWNER),
    )
    .unwrap();
    assert_eq!(claimed, ids(&[STALE]));
    assert!(!view[0].awaiting_owner);
    assert!(view[1].awaiting_owner);
    assert_eq!(view[1].owner_account_id.as_deref(), Some(SECOND));
    assert_eq!(view[1].offset, Some(STALE_OFFSET));
}

#[test]
fn explicit_pre_dispatch_failure_restores_the_original_deferred_retry() {
    for binding in [Some(OWNER), Some(SECOND), None] {
        let mut manifest = vec![deferred(STALE, OWNER)];
        let (_, claimed) = checkpoint(
            &mut manifest,
            &[STALE],
            RecoveryMode::ExplicitTarget,
            binding,
        )
        .unwrap();
        let preserve = claimed.contains(&STALE.to_string());
        finalize_target(&mut manifest, STALE, true, false, binding, preserve);
        assert_eq!(manifest.len(), 1);
        assert_original_deferred(&manifest[0]);
    }
    // Once a request may have reached Desktop, the intent is gone for good.
    let mut manifest = vec![deferred(STALE, OWNER)];
    finalize_target(&mut manifest, STALE, false, true, Some(OWNER), true);
    assert!(manifest.is_empty());
}

#[test]
fn explicit_resume_refreshes_an_unbound_checkpoint() {
    let mut manifest = vec![PendingTarget {
        awaiting_owner: false,
        owner_account_id: None,
        ..deferred(STALE, OWNER)
    }];
    let (view, claimed) =
        checkpoint(&mut manifest, &[STALE], RecoveryMode::ExplicitTarget, None).unwrap();
    assert!(claimed.is_empty());
    assert_eq!(manifest[0].offset, Some(FRESH_OFFSET));
    assert_eq!(view[0].offset, Some(FRESH_OFFSET));
}

#[test]
fn checkpoint_adds_new_targets_and_rejects_invalid_ids_before_mutation() {
    let mut manifest = Vec::new();
    checkpoint(&mut manifest, &[OTHER], RecoveryMode::CapturedRestart, None).unwrap();
    assert_eq!(manifest.len(), 1);
    assert!(manifest[0].captured_restart);
    assert!(!manifest[0].awaiting_owner);
    assert_eq!(manifest[0].offset, Some(FRESH_OFFSET));

    let mut manifest = vec![deferred(STALE, OWNER)];
    let error = checkpoint(
        &mut manifest,
        &[STALE, "../not-a-thread"],
        RecoveryMode::ExplicitTarget,
        Some(OWNER),
    )
    .unwrap_err();
    assert_eq!(error, "Invalid thread ID");
    assert_eq!(manifest.len(), 1);
    assert_original_deferred(&manifest[0]);
}
