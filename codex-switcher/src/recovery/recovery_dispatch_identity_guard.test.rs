use super::RecoveryDispatchIdentityGuard;
use crate::{
    distribution::WindowProcessIdentity,
    recovery::{RecoveryBanner, RecoveryMode},
};
use std::cell::Cell;

#[test]
fn hidden_banner_must_refer_to_the_exact_live_process_at_capture() {
    let expected = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let banner = RecoveryBanner::without_window(expected.clone());
    let later_birth = WindowProcessIdentity::new(4242, "1726789012:000008").unwrap();
    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::ExplicitTarget,
        || Some("account-a".into()),
        || Ok(later_birth.clone()),
        |_| Some(expected.clone()),
    )
    .is_err());
    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::ExplicitTarget,
        || Some("account-a".into()),
        || Ok(expected.clone()),
        |_| Some(expected.clone()),
    )
    .is_ok());
}

#[test]
fn captured_restart_with_hidden_banner_pins_the_relaunched_desktop() {
    let old = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let relaunched = WindowProcessIdentity::new(5252, "1726789112:000001").unwrap();
    let banner = RecoveryBanner::without_window(old);
    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::CapturedRestart,
        || Some("account-a".into()),
        || Ok(relaunched.clone()),
        |_| Some(relaunched.clone()),
    )
    .is_ok());
    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::CapturedRestart,
        || Some("account-a".into()),
        || Ok(relaunched.clone()),
        |_| None,
    )
    .is_err());
    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::DeferredCaptured,
        || Some("account-a".into()),
        || Ok(relaunched.clone()),
        |_| Some(relaunched.clone()),
    )
    .is_err());

    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::CapturedRestart,
        || Some("account-a".into()),
        || Ok(relaunched.clone()),
        |_| Some(WindowProcessIdentity::new(4242, "1726789012:000007").unwrap()),
    )
    .is_err());
    let marker_reads = Cell::new(0);
    assert!(RecoveryDispatchIdentityGuard::capture_with(
        &banner,
        RecoveryMode::CapturedRestart,
        || Some("account-a".into()),
        || Ok(relaunched.clone()),
        |_| {
            marker_reads.set(marker_reads.get() + 1);
            if marker_reads.get() == 1 {
                Some(relaunched.clone())
            } else {
                Some(WindowProcessIdentity::new(4242, "1726789012:000007").unwrap())
            }
        },
    )
    .is_err());
    assert_eq!(marker_reads.get(), 2);
}

#[test]
fn dispatch_guard_rechecks_account_and_process_after_a_wait() {
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let guard = RecoveryDispatchIdentityGuard {
        account_id: "account-a".into(),
        process: process.clone(),
    };
    let accounts = Cell::new(0);
    assert!(guard
        .verify_with(
            || {
                accounts.set(accounts.get() + 1);
                Some(if accounts.get() == 1 {
                    "account-a".into()
                } else {
                    "account-b".into()
                })
            },
            || Ok(process.clone()),
        )
        .is_err());
    assert_eq!(accounts.get(), 2);
    assert!(guard
        .verify_with(
            || Some("account-a".into()),
            || Ok(WindowProcessIdentity::new(4242, "1726789012:000008").unwrap()),
        )
        .is_err());
    assert!(guard
        .verify_with(|| Some("account-a".into()), || Ok(process.clone()))
        .is_ok());
}

#[test]
fn production_dispatch_checks_consult_the_live_desktop() {
    // The live seams refuse to run in unit tests, so reaching each one proves
    // the production wiring without touching the owner's Desktop: the identity
    // recheck reads the ChatGPT process table, and the deferred binding asks
    // the window helper to verify the Desktop session rather than trusting
    // the CLI auth alone.
    let env = crate::distribution::test_helper::TestEnv::new("dispatch_checks_wiring");
    let account = crate::distribution::test_account_spec::TestAccountSpec {
        id: "account-a",
        email: "first@example.test",
        plan: "plus",
        ..crate::distribution::test_account_spec::TestAccountSpec::default()
    }
    .build();
    env.populate(vec![account], Some("account-a"), Some("account-a"));
    let guard = RecoveryDispatchIdentityGuard {
        account_id: super::current_account_binding().expect("seeded CLI auth must bind"),
        process: WindowProcessIdentity::new(4242, "1726789012:000007").unwrap(),
    };

    crate::test_live_system::assert_forbidden("process table (/bin/ps)", || {
        guard.dispatch_checks().verify()
    });
    crate::test_live_system::assert_forbidden("installed window-restore helper", || {
        guard.dispatch_checks().deferred_binding()
    });
    drop(env);
}
