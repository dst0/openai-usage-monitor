use super::{eligible_mount, mount_with_banner};
use crate::recovery::pending_target::PendingTarget;
use crate::switcher::ThreadRolloutState;
use std::cell::{Cell, RefCell};

fn target(account: &str) -> PendingTarget {
    PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
        offset: Some(84),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some(account.into()),
    }
}

#[test]
fn pending_panel_candidate_requires_same_account_and_dispatchable_work() {
    let item = target("account-a");
    assert!(eligible_mount(
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        &item,
        "account-b",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByError,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        None
    ));
    assert!(!eligible_mount(
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((true, false))
    ));
    assert!(!eligible_mount(
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, true))
    ));
    let mut not_ownerless = item.clone();
    not_ownerless.awaiting_owner = false;
    assert!(!eligible_mount(
        &not_ownerless,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, false))
    ));
}

#[test]
fn panel_is_pending_during_mount_and_same_instance_reaches_recovery() {
    let events = RefCell::new(Vec::new());
    let owner_calls = Cell::new(0);
    let mut panel = 7u32;
    let mounted = mount_with_banner(
        &mut panel,
        |_| {
            events.borrow_mut().push("verify");
            Ok(())
        },
        |_| {
            events.borrow_mut().push("show_pending");
            Ok(true)
        },
        || {
            events.borrow_mut().push("navigate");
            Ok(())
        },
        || {
            owner_calls.set(owner_calls.get() + 1);
            Ok(Some(owner_calls.get() == 2))
        },
        |panel| {
            assert_eq!(*panel, 7);
            *panel = 8;
            events.borrow_mut().push("recover");
            Ok(())
        },
    )
    .unwrap();
    assert!(mounted);
    assert_eq!(panel, 8);
    assert_eq!(events.borrow().as_slice().last(), Some(&"recover"));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|item| **item == "show_pending")
            .count(),
        3
    );
}

#[test]
fn timeout_or_identity_change_never_reaches_recovery() {
    for identity_fails in [false, true] {
        let verifies = Cell::new(0);
        let recovered = Cell::new(false);
        let mut panel = ();
        let result = mount_with_banner(
            &mut panel,
            |_| {
                verifies.set(verifies.get() + 1);
                if identity_fails && verifies.get() > 1 {
                    Err("account changed".into())
                } else {
                    Ok(())
                }
            },
            |_| Ok(true),
            || Ok(()),
            || Ok(None),
            |_| {
                recovered.set(true);
                Ok(())
            },
        );
        assert!(!recovered.get());
        assert_eq!(result.is_err(), identity_fails);
        if !identity_fails {
            assert!(!result.unwrap());
        }
    }
}

#[test]
fn late_window_must_be_visible_before_recovery() {
    let shows = Cell::new(0);
    let mut panel = ();
    let result = mount_with_banner(
        &mut panel,
        |_| Ok(()),
        |_| {
            shows.set(shows.get() + 1);
            Ok(false)
        },
        || Ok(()),
        || Ok(Some(true)),
        |_| panic!("recovery without visible panel"),
    );
    assert!(result.is_err());
    assert!(shows.get() >= 2);
}

#[test]
fn ownerless_without_initial_window_navigates_then_keeps_late_panel_for_recovery() {
    let events = RefCell::new(Vec::new());
    let navigated = Cell::new(false);
    let shows = Cell::new(0);
    let owner_calls = Cell::new(0);
    let mut panel = 17u32;
    let mounted = mount_with_banner(
        &mut panel,
        |_| Ok(()),
        |_| {
            shows.set(shows.get() + 1);
            let visible = navigated.get() && shows.get() >= 3;
            events.borrow_mut().push(if visible {
                "panel_visible"
            } else {
                "no_window"
            });
            Ok(visible)
        },
        || {
            events.borrow_mut().push("url_sent");
            navigated.set(true);
            Ok(())
        },
        || {
            owner_calls.set(owner_calls.get() + 1);
            Ok(Some(owner_calls.get() == 2))
        },
        |same_panel| {
            assert_eq!(*same_panel, 17);
            events.borrow_mut().push("recover");
            Ok(())
        },
    )
    .unwrap();
    assert!(mounted);
    assert_eq!(
        *events.borrow(),
        [
            "no_window",
            "url_sent",
            "no_window",
            "panel_visible",
            "recover"
        ]
    );
}
