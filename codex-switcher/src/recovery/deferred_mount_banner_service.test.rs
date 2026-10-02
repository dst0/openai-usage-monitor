use super::{eligible_mount, mount_with_banner};
use crate::recovery::auth_rotation_queue_snapshot::AuthRotationQueueSnapshot;
use crate::recovery::auth_rotation_recovery_evidence::AuthRotationRecoveryEvidence;
use crate::recovery::pending_target::PendingTarget;
use crate::storage::test_codex_home::TestCodexHome;
use crate::switcher::ThreadRolloutState;
use std::cell::{Cell, RefCell};
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};

fn target(account: &str) -> PendingTarget {
    PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
        offset: Some(84),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some(account.into()),
        auth_rotation: None,
    }
}

#[test]
fn pending_panel_candidate_requires_same_account_and_dispatchable_work() {
    let item = target("account-a");
    let home = Path::new("/nonexistent-codex-home");
    assert!(eligible_mount(
        home,
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        home,
        &item,
        "account-b",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        home,
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByError,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        home,
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        None
    ));
    assert!(!eligible_mount(
        home,
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((true, false))
    ));
    assert!(!eligible_mount(
        home,
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, true))
    ));
    let mut not_ownerless = item.clone();
    not_ownerless.awaiting_owner = false;
    assert!(!eligible_mount(
        home,
        &not_ownerless,
        "account-a",
        ThreadRolloutState::InterruptedByQuota,
        0,
        Some((false, false))
    ));
}

#[test]
fn confirmed_auth_error_can_mount_only_while_its_queue_snapshot_is_current() {
    let env = TestCodexHome::new("deferred-auth-rotation-mount");
    let mut item = target("account-b");
    let sessions = env.path().join("sessions");
    fs::create_dir(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-{}.jsonl", item.id));
    let started = serde_json::json!({
        "type": "event_msg",
        "payload": {"type": "task_started", "turn_id": "turn-a"}
    })
    .to_string();
    let ended = serde_json::json!({
        "type": "event_msg",
        "payload": {
            "type": "task_complete",
            "turn_id": "turn-a",
            "error": {"message": "Your access token could not be refreshed because you have since logged out or signed in to another account. Please sign in again."}
        }
    })
    .to_string();
    fs::write(&rollout, format!("{started}\n{ended}\n")).unwrap();
    let metadata = rollout.metadata().unwrap();
    item.offset = Some(metadata.len());
    item.auth_rotation = Some(AuthRotationRecoveryEvidence {
        source_account_id: "account-a".into(),
        target_account_id: "account-b".into(),
        pre_stop_offset: started.len() as u64 + 1,
        rollout_dev: metadata.dev(),
        rollout_ino: metadata.ino(),
        turn_id: "turn-a".into(),
        queue_snapshot: AuthRotationQueueSnapshot::read(env.path(), &item.id).unwrap(),
        confirmed_after_stop: true,
    });
    assert!(eligible_mount(
        env.path(),
        &item,
        "account-b",
        ThreadRolloutState::InterruptedByError,
        0,
        Some((false, false))
    ));
    assert!(!eligible_mount(
        env.path(),
        &item,
        "account-a",
        ThreadRolloutState::InterruptedByError,
        0,
        Some((false, false))
    ));
    let queue = env.path().join("queue_1.sqlite");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg("CREATE TABLE queued_items (thread_id TEXT); CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER);")
        .status()
        .unwrap()
        .success());
    assert!(!eligible_mount(
        env.path(),
        &item,
        "account-b",
        ThreadRolloutState::InterruptedByError,
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
