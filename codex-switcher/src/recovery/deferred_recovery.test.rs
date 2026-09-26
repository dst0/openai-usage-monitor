use super::{
    deferred_recovery_service::{
        probe_or_retry_navigation, record_navigation_attempt, select_ready_targets,
        select_scanned_ready_targets, should_retry_navigation, tracked_probe,
        DeferredNavigationRoute,
    },
    ipc_call_error::IpcCallError,
    pending_target::PendingTarget,
};
use crate::storage::test_codex_home::TestCodexHome;
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
fn navigation_routes_alternate_after_an_attempt() {
    assert_eq!(
        DeferredNavigationRoute::after(None),
        DeferredNavigationRoute::Ordinary
    );
    assert_eq!(
        DeferredNavigationRoute::after(Some(DeferredNavigationRoute::Ordinary)),
        DeferredNavigationRoute::PinnedNative
    );
    assert_eq!(
        DeferredNavigationRoute::after(Some(DeferredNavigationRoute::PinnedNative)),
        DeferredNavigationRoute::Ordinary
    );
}

#[test]
fn failed_ordinary_delivery_selects_native_on_next_probe() {
    let mut attempted = None;
    let at = Instant::now();
    record_navigation_attempt(&mut attempted, DeferredNavigationRoute::Ordinary, at);
    assert_eq!(attempted, Some((at, DeferredNavigationRoute::Ordinary)));
    assert_eq!(
        DeferredNavigationRoute::after(attempted.map(|(_, route)| route)),
        DeferredNavigationRoute::PinnedNative,
    );
}

#[test]
fn failed_delivery_still_obeys_one_minute_attempt_cooldown() {
    let _home = TestCodexHome::new("deferred-failed-cooldown");
    let at = Instant::now();
    let mut attempted = None;
    assert!(!probe_or_retry_navigation(
        || Err(IpcCallError::NoClientFound),
        || {
            record_navigation_attempt(&mut attempted, DeferredNavigationRoute::Ordinary, at);
            Err("ordinary launch failed".into())
        },
        true,
    )
    .unwrap());
    let attempt_at = attempted.map(|(at, _)| at);
    assert!(!should_retry_navigation(
        attempt_at,
        at + Duration::from_secs(15)
    ));
    assert!(should_retry_navigation(
        attempt_at,
        at + Duration::from_secs(60)
    ));
}

#[test]
fn changed_desktop_identity_stops_deferred_navigation() {
    assert!(probe_or_retry_navigation(
        || Err(IpcCallError::NoClientFound),
        || Err("ChatGPT process identity changed during task navigation".into()),
        true,
    )
    .is_err());
}

#[test]
fn first_navigation_attempt_timestamp_survives_later_errors() {
    let first_at = Instant::now();
    let mut attempted = None;
    record_navigation_attempt(&mut attempted, DeferredNavigationRoute::Ordinary, first_at);
    assert_eq!(
        attempted,
        Some((first_at, DeferredNavigationRoute::Ordinary))
    );
    let accepted_at = first_at + Duration::from_secs(1);
    record_navigation_attempt(
        &mut attempted,
        DeferredNavigationRoute::Ordinary,
        accepted_at,
    );
    record_navigation_attempt(
        &mut attempted,
        DeferredNavigationRoute::Ordinary,
        accepted_at + Duration::from_secs(1),
    );
    assert_eq!(
        attempted,
        Some((first_at, DeferredNavigationRoute::Ordinary))
    );
    let (result, retained) = tracked_probe(|navigation| {
        record_navigation_attempt(navigation, DeferredNavigationRoute::Ordinary, accepted_at);
        Err("rollout scan failed after link delivery".into())
    });
    assert!(result.is_err());
    assert_eq!(
        retained,
        Some((accepted_at, DeferredNavigationRoute::Ordinary))
    );
}

#[test]
fn ownerless_deferred_target_reissues_link_without_dispatch() {
    let _home = TestCodexHome::new("deferred-ownerless");
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
    assert!(!probe_or_retry_navigation(
        || Err(IpcCallError::NoClientFound),
        || Err("LaunchServices unavailable".into()),
        true,
    )
    .unwrap());
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
fn failed_link_delivery_does_not_hide_a_later_owned_target() {
    let _home = TestCodexHome::new("deferred-navigation-failure");
    let targets: Vec<_> = ["e80", "e81"]
        .iter()
        .map(|suffix| PendingTarget {
            id: format!("01a098c2-0fae-74d2-a80c-45d89e910{suffix}"),
            offset: Some(84),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        })
        .collect();
    let mut probed = 0;
    let ready = select_scanned_ready_targets(
        &targets,
        "account-a",
        |_| {
            probed += 1;
            probe_or_retry_navigation(
                || {
                    if probed == 1 {
                        Err(IpcCallError::NoClientFound)
                    } else {
                        Ok(())
                    }
                },
                || Err("LaunchServices unavailable".into()),
                true,
            )
        },
        |_, _| Ok(true),
    )
    .unwrap();
    assert_eq!(probed, 2);
    assert_eq!(ready, [targets[1].id.clone()]);
}

#[test]
fn eligible_probe_reissues_a_link_for_each_ownerless_target() {
    let _home = TestCodexHome::new("deferred-per-target-navigation");
    let targets: Vec<_> = ["e80", "e81"]
        .iter()
        .map(|suffix| PendingTarget {
            id: format!("01a098c2-0fae-74d2-a80c-45d89e910{suffix}"),
            offset: Some(84),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        })
        .collect();
    let mut navigated = Vec::new();
    let ready = select_scanned_ready_targets(
        &targets,
        "account-a",
        |id| {
            probe_or_retry_navigation(
                || Err(IpcCallError::NoClientFound),
                || {
                    navigated.push(id.to_string());
                    if navigated.len() == 1 {
                        Err("first delivery failed".into())
                    } else {
                        Ok(())
                    }
                },
                true,
            )
        },
        |_, _| panic!("ownerless targets cannot start a rollout scan"),
    )
    .unwrap();
    assert!(ready.is_empty());
    assert_eq!(navigated, [targets[0].id.clone(), targets[1].id.clone()]);
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

#[test]
fn owner_probe_precedes_bounded_rollout_scan() {
    let target = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e80".into(),
        offset: Some(84),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let no_owner = select_scanned_ready_targets(
        std::slice::from_ref(&target),
        "account-a",
        |_| Ok(false),
        |_, _| panic!("cold task without an owner scanned its rollout"),
    )
    .unwrap();
    assert!(no_owner.is_empty());

    let mut observed_budget = 0;
    let ready = select_scanned_ready_targets(
        std::slice::from_ref(&target),
        "account-a",
        |_| Ok(true),
        |_, budget| {
            observed_budget = *budget;
            Ok(true)
        },
    )
    .unwrap();
    assert_eq!(ready, [target.id]);
    assert_eq!(observed_budget, 16 * 1024 * 1024);
}
