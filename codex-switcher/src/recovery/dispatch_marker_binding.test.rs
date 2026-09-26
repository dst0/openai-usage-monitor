//! The deferred-account gate that `mark_dispatch_attempt` applies immediately
//! before it consumes a recovery checkpoint, across every recovery mode.

use super::{
    dispatch_identity_checks::DispatchIdentityChecks,
    dispatch_mark_error::DispatchMarkError,
    manifest_store::{load_manifest, mark_dispatch_attempt, write_manifest},
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
};
use crate::storage::test_codex_home::TestCodexHome;
use std::cell::Cell;

const ID: &str = "01a098c2-0fae-74d2-a80c-45d89e910e79";
const MODES: [RecoveryMode; 5] = [
    RecoveryMode::CapturedRestart,
    RecoveryMode::DeferredOwned,
    RecoveryMode::DeferredCaptured,
    RecoveryMode::ExplicitTarget,
    RecoveryMode::DiscoveredOnly,
];

#[derive(Debug, PartialEq)]
enum Marked {
    Consumed,
    AccountChanged,
}

struct Attempt {
    marked: Marked,
    retained: bool,
    binding_reads: usize,
    identity_checks: usize,
}

/// Marks a checkpoint saved by `account-a` while the verified Desktop session
/// resolves to `desktop_account`.
fn mark(mode: RecoveryMode, awaiting_owner: bool, desktop_account: Option<&str>) -> Attempt {
    let _home = TestCodexHome::new("dispatch-marker-binding");
    let original = PendingTarget {
        id: ID.into(),
        offset: Some(42),
        awaiting_owner,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    write_manifest(std::slice::from_ref(&original)).unwrap();
    let (binding_reads, identity_checks) = (Cell::new(0), Cell::new(0));
    let mut identity = DispatchIdentityChecks::new(
        || {
            identity_checks.set(identity_checks.get() + 1);
            Ok(())
        },
        || {
            binding_reads.set(binding_reads.get() + 1);
            desktop_account.map(str::to_owned)
        },
    );
    let marked = match mark_dispatch_attempt(ID, mode, &mut identity) {
        Ok(consumed) => {
            assert_eq!(consumed, original);
            Marked::Consumed
        }
        Err(DispatchMarkError::AccountChanged) => Marked::AccountChanged,
        Err(error) => panic!("unexpected marker error: {error}"),
    };
    let retained = load_manifest().unwrap() == vec![original];
    drop(identity);
    Attempt {
        marked,
        retained,
        binding_reads: binding_reads.get(),
        identity_checks: identity_checks.get(),
    }
}

#[test]
fn only_unattended_modes_bind_an_awaiting_owner_checkpoint_to_its_desktop_account() {
    for mode in MODES {
        for awaiting_owner in [false, true] {
            let bound = awaiting_owner && mode != RecoveryMode::ExplicitTarget;
            for desktop_account in [Some("account-a"), Some("account-b"), None] {
                let attempt = mark(mode, awaiting_owner, desktop_account);
                let case =
                    format!("{mode:?} awaiting_owner={awaiting_owner} desktop={desktop_account:?}");
                assert_eq!(attempt.binding_reads, usize::from(bound), "{case}");
                let refused = bound && desktop_account != Some("account-a");
                if refused {
                    assert_eq!(attempt.marked, Marked::AccountChanged, "{case}");
                    assert!(attempt.retained, "{case}: checkpoint must survive");
                    assert_eq!(attempt.identity_checks, 0, "{case}");
                } else {
                    assert_eq!(attempt.marked, Marked::Consumed, "{case}");
                    assert!(!attempt.retained, "{case}: checkpoint must be consumed");
                    assert_eq!(attempt.identity_checks, 1, "{case}");
                }
            }
        }
    }
}

#[test]
fn a_changed_operation_identity_keeps_the_checkpoint_in_every_mode() {
    for mode in MODES {
        let _home = TestCodexHome::new("dispatch-marker-identity");
        let original = PendingTarget {
            id: ID.into(),
            offset: Some(42),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        };
        write_manifest(std::slice::from_ref(&original)).unwrap();
        let mut identity = DispatchIdentityChecks::new(
            || Err(DispatchMarkError::AccountChanged),
            || Some("account-a".into()),
        );

        let result = mark_dispatch_attempt(ID, mode, &mut identity);

        assert!(
            matches!(result, Err(DispatchMarkError::AccountChanged)),
            "{mode:?}: {result:?}"
        );
        assert_eq!(load_manifest().unwrap(), vec![original], "{mode:?}");
    }
}
