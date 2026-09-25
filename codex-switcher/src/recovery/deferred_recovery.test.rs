use super::{
    deferred_recovery_service::{
        probe_or_retry_navigation, select_ready_targets, should_retry_navigation,
    },
    ipc_call_error::IpcCallError,
    pending_target::PendingTarget,
};
use std::time::{Duration, Instant};

#[test]
fn background_navigation_retry_has_one_minute_cooldown() {
    let started = Instant::now();
    assert!(should_retry_navigation(None, started));
    assert!(!should_retry_navigation(
        Some(started),
        started + Duration::from_secs(59),
    ));
    assert!(should_retry_navigation(
        Some(started),
        started + Duration::from_secs(60),
    ));
}

#[test]
fn ownerless_deferred_target_reissues_link_without_dispatch() {
    let mut launches = 0;
    assert!(!probe_or_retry_navigation(
        || Err(IpcCallError::NoClientFound),
        || {
            launches += 1;
            Ok(())
        },
        true,
    )
    .unwrap());
    assert_eq!(launches, 1);
    assert!(!probe_or_retry_navigation(
        || Err(IpcCallError::NoClientFound),
        || panic!("navigation cooldown ignored"),
        false,
    )
    .unwrap());
    assert!(probe_or_retry_navigation(
        || Ok(()),
        || panic!("an owned task was opened again"),
        true,
    )
    .unwrap());
    assert!(probe_or_retry_navigation(
        || Err(IpcCallError::NoClientFound),
        || Err("LaunchServices unavailable".into()),
        true,
    )
    .is_err());
}

#[test]
fn only_explicitly_ownerless_targets_with_a_current_owner_are_retried() {
    let targets = [
        PendingTarget {
            id: "01a098c2-0fae-74d2-a80c-45d89e910e79".into(),
            offset: Some(42),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
            offset: Some(84),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        },
        PendingTarget {
            id: "01a098c2-0fae-74d2-a80c-45d89e910e81".into(),
            offset: Some(126),
            awaiting_owner: true,
            captured_restart: false,
            owner_account_id: Some("account-a".into()),
        },
    ];
    let mut probed = Vec::new();
    let ready = select_ready_targets(&targets, "account-a", |id| {
        probed.push(id.to_string());
        Ok(id.ends_with("e80"))
    })
    .unwrap();
    assert_eq!(ready, [targets[1].id.clone()]);
    assert_eq!(probed, [targets[1].id.clone(), targets[2].id.clone()]);
}

#[test]
fn owner_probe_error_cannot_schedule_a_turn() {
    let targets = [PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
        offset: Some(84),
        awaiting_owner: true,
        captured_restart: false,
        owner_account_id: Some("account-a".into()),
    }];
    assert!(
        select_ready_targets(&targets, "account-a", |_| Err("IPC unavailable".into())).is_err()
    );
}

#[test]
fn account_change_does_not_probe_or_retry_deferred_target() {
    let target = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
        offset: Some(84),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let ready =
        select_ready_targets(&[target], "account-b", |_| panic!("wrong account probed")).unwrap();
    assert!(ready.is_empty());
}
