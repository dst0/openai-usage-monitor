use super::{CheckpointScanRegistry, ManifestPruneService, OwnerlessProbeRotation, PendingTarget};
use chrono::{TimeZone, Utc};

fn quota_fixture(
    label: &str,
    failed_at: Option<i64>,
    awaiting_owner: bool,
) -> (std::path::PathBuf, PendingTarget) {
    let home = std::env::temp_dir().join(format!(
        "codex-prune-quota-{label}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let rollout = sessions.join(format!("rollout-2026-09-27T00-00-00-{id}.jsonl"));
    let mut event = serde_json::json!({
        "timestamp": failed_at.map(|time| Utc.timestamp_opt(time, 0).single().unwrap().to_rfc3339()),
        "type": "event_msg",
        "payload": {"type": "task_complete", "turn_id": "quota-turn", "last_agent_message": null,
            "error": {"message": "usage_limit_exceeded"}}
    });
    if failed_at.is_none() {
        event.as_object_mut().unwrap().remove("timestamp");
    }
    std::fs::write(&rollout, format!("{event}\n")).unwrap();
    let offset = std::fs::metadata(&rollout).unwrap().len();
    (
        home,
        PendingTarget {
            id: id.into(),
            offset: Some(offset),
            awaiting_owner,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        },
    )
}

#[test]
fn quota_manifest_pruning_uses_failure_time_even_when_sqlite_moves() {
    let now = Utc::now().timestamp();
    for awaiting_owner in [false, true] {
        for (label, failed_at, updated_at, expected) in [
            ("old-reopened", Some(now - 5 * 3600), now, false),
            ("recent-stale-row", Some(now - 30), now - 5 * 3600, true),
            ("missing-time", None, now, false),
        ] {
            let (home, target) = quota_fixture(label, failed_at, awaiting_owner);
            let mut pending = vec![target.clone()];
            // A fresh rotation selects this home's only ownerless target, so
            // the deferred path dates it instead of keeping it unexamined.
            let rotation = OwnerlessProbeRotation::new(1);
            let scans = CheckpointScanRegistry::new(1);
            let result =
                ManifestPruneService::new(&rotation, &scans)
                    .run_with(&home, &mut pending, |_| Ok(Some(updated_at)));
            std::fs::remove_dir_all(&home).unwrap();
            if awaiting_owner && failed_at.is_none() {
                assert!(
                    result.is_err(),
                    "missing quota time must block deferred dispatch"
                );
                assert_eq!(pending, vec![target], "uncertain checkpoint stays intact");
                continue;
            }
            result.unwrap();
            assert_eq!(
                pending.len(),
                usize::from(expected),
                "{label} awaiting_owner={awaiting_owner}"
            );
        }
    }
}

#[test]
fn rollout_append_during_prune_retains_checkpoint_for_retry() {
    use std::io::Write;
    let now = Utc::now().timestamp();
    for awaiting_owner in [false, true] {
        let (home, target) = quota_fixture("concurrent-append", Some(now - 30), awaiting_owner);
        let rollout = home
            .join("sessions")
            .join(format!("rollout-2026-09-27T00-00-00-{}.jsonl", target.id));
        let mut pending = vec![target.clone()];
        let rotation = OwnerlessProbeRotation::new(1);
        let scans = CheckpointScanRegistry::new(1);
        let result = ManifestPruneService::new(&rotation, &scans).run_with_inspector(
            &home,
            &mut pending,
            |_| Ok(Some(now - 5 * 3600)),
            |_, _| {
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&rollout)
                    .unwrap();
                file.write_all(
                    b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",
                )
                .unwrap();
                crate::switcher::ThreadRolloutState::InterruptedByQuota
            },
        );
        std::fs::remove_dir_all(&home).unwrap();
        assert!(result.is_err(), "changed rollout must defer pruning");
        assert_eq!(pending, vec![target]);
    }
}

#[test]
fn rollout_append_after_timestamp_read_blocks_prune() {
    use std::io::Write;
    let now = Utc::now().timestamp();
    let (home, target) = quota_fixture("append-after-time", Some(now - 30), true);
    let rollout = home
        .join("sessions")
        .join(format!("rollout-2026-09-27T00-00-00-{}.jsonl", target.id));
    let before = std::fs::symlink_metadata(&rollout).unwrap();
    let result = ManifestPruneService::checked_quota_recency_with(
        &home,
        &target.id,
        now,
        &rollout,
        &before,
        |home, id| {
            let timestamp = crate::switcher::quota_failure_timestamp(home, id);
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&rollout)
                .unwrap();
            file.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n")
                .unwrap();
            timestamp
        },
    );
    std::fs::remove_dir_all(&home).unwrap();
    assert!(result.is_err(), "second guard must reject a later append");
}

static NEXT_ROTATION_HOME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn rotation_home(name: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!(
        "codex-prune-service-test-{name}-{}-{}",
        std::process::id(),
        NEXT_ROTATION_HOME.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&home).expect("scratch home must be new");
    std::fs::create_dir(home.join("sessions")).unwrap();
    home
}

/// An ownerless target whose checkpoint is followed by an unfinished turn,
/// so a pass that selects it leaves a cursor in the scan registry.
fn ownerless(home: &std::path::Path, index: u64) -> (PendingTarget, std::path::PathBuf) {
    let id = format!("01a098c2-0fae-74d2-a80c-{:012x}", 0xf000 + index);
    let rollout = home
        .join("sessions")
        .join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"t\"}}\n",
    )
    .unwrap();
    let target = PendingTarget {
        id,
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    };
    (target, rollout)
}

fn scanned(scans: &CheckpointScanRegistry, rollouts: &[&std::path::PathBuf]) -> Vec<bool> {
    rollouts
        .iter()
        .map(|rollout| scans.scanned_bytes_for(rollout, 11).is_some())
        .collect()
}

/// A rotation whose cursor for `home` has been advanced `turns` times over
/// `count` ownerless targets.
fn rotation_at(home: &std::path::Path, turns: usize, count: usize) -> OwnerlessProbeRotation {
    let rotation = OwnerlessProbeRotation::new(4);
    for _ in 0..turns {
        rotation.select(home, count);
    }
    rotation
}

#[test]
fn invalid_or_unindexed_ownerless_target_does_not_waste_the_scan_turn() {
    let home = rotation_home("unselectable-turn");
    let [(unindexed, _), (first, first_rollout), (second, second_rollout)] =
        [1, 2, 3].map(|index| ownerless(&home, index));
    let invalid = PendingTarget {
        id: "not-a-thread-id".into(),
        ..unindexed.clone()
    };
    let now = Utc::now().timestamp();
    let unindexed_id = unindexed.id.clone();
    let updated_at = |id: &str| Ok((id != unindexed_id).then_some(now));
    // Neither an invalid ID nor a thread missing from SQLite can be scanned,
    // so neither may hold a rotation index. Counting them made the cursor
    // select them and the pass scan nothing. Cursors 2 and 3 are left over
    // from a pass over four targets. The cursor starts away from 0 so a
    // service that ignored the injected rotation would scan `first`.
    let cases = [
        (1, 2, [false, true]),
        (3, 4, [false, true]),
        (2, 4, [true, false]),
        (0, 2, [true, false]),
    ];
    for (turns, count, expected) in cases {
        let rotation = rotation_at(&home, turns, count);
        let scans = CheckpointScanRegistry::new(4);
        let mut targets = vec![
            invalid.clone(),
            unindexed.clone(),
            first.clone(),
            second.clone(),
        ];
        ManifestPruneService::new(&rotation, &scans)
            .run_with(&home, &mut targets, updated_at)
            .unwrap();
        assert_eq!(targets, [first.clone(), second.clone()], "turns {turns}");
        assert_eq!(
            scanned(&scans, &[&first_rollout, &second_rollout]),
            expected,
            "turns {turns}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn pass_whose_ownerless_targets_are_all_unselectable_keeps_the_rotation_turn() {
    let home = rotation_home("all-unselectable");
    let [(unindexed, _), (first, first_rollout), (second, second_rollout)] =
        [1, 2, 3].map(|index| ownerless(&home, index));
    let invalid = PendingTarget {
        id: "not-a-thread-id".into(),
        ..unindexed.clone()
    };
    let now = Utc::now().timestamp();
    let rotation = rotation_at(&home, 1, 2);
    let scans = CheckpointScanRegistry::new(4);
    let service = ManifestPruneService::new(&rotation, &scans);
    let mut unselectable = vec![invalid, unindexed];
    service
        .run_with(&home, &mut unselectable, |_| Ok(None))
        .unwrap();
    assert!(unselectable.is_empty());
    // No selectable ownerless target took the turn, so the next real pass
    // over this home still scans the second target.
    let mut targets = vec![first, second];
    service
        .run_with(&home, &mut targets, |_| Ok(Some(now)))
        .unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(
        scanned(&scans, &[&first_rollout, &second_rollout]),
        [false, true]
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn restart_targets_do_not_shift_the_ownerless_rotation() {
    let home = rotation_home("mixed-pass");
    let [(first, first_rollout), (second, second_rollout)] =
        [1, 2].map(|index| ownerless(&home, index));
    // A restart target without a rollout is dropped; it must not take an
    // ownerless index, or the cursor would land on it and scan nothing.
    let restart = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-00000000f0ff".into(),
        awaiting_owner: false,
        owner_account_id: None,
        ..first.clone()
    };
    let now = Utc::now().timestamp();
    for (turns, expected) in [(1, [false, true]), (0, [true, false])] {
        let rotation = rotation_at(&home, turns, 2);
        let scans = CheckpointScanRegistry::new(4);
        let mut targets = vec![restart.clone(), first.clone(), second.clone()];
        ManifestPruneService::new(&rotation, &scans)
            .run_with(&home, &mut targets, |_| Ok(Some(now)))
            .unwrap();
        assert_eq!(targets, [first.clone(), second.clone()], "turns {turns}");
        assert_eq!(
            scanned(&scans, &[&first_rollout, &second_rollout]),
            expected,
            "turns {turns}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_thread_index_lookup_does_not_consume_the_rotation_turn() {
    let home = rotation_home("index-error");
    let [(first, first_rollout), (second, second_rollout)] =
        [1, 2].map(|index| ownerless(&home, index));
    let rotation = rotation_at(&home, 0, 2);
    let scans = CheckpointScanRegistry::new(4);
    let service = ManifestPruneService::new(&rotation, &scans);
    let mut targets = vec![first.clone(), second.clone()];
    let second_id = second.id.clone();
    let result = service.run_with(&home, &mut targets, |id| {
        if id == second_id {
            Err("SQLite temporarily unavailable".into())
        } else {
            Ok(Some(Utc::now().timestamp()))
        }
    });
    assert!(result.is_err());
    assert_eq!(targets, [first.clone(), second.clone()], "nothing pruned");
    assert_eq!(
        scanned(&scans, &[&first_rollout, &second_rollout]),
        [false, false]
    );
    // The aborted pass selected nothing, so the next pass takes the first turn.
    service
        .run_with(&home, &mut targets, |_| Ok(Some(Utc::now().timestamp())))
        .unwrap();
    assert_eq!(
        scanned(&scans, &[&first_rollout, &second_rollout]),
        [true, false]
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// Replaces `target`'s rollout with a quota failure at `failed_at`, and moves
/// its checkpoint to the end so no post-checkpoint work is scanned.
fn fail_on_quota(rollout: &std::path::Path, target: &mut PendingTarget, failed_at: i64) {
    let event = serde_json::json!({
        "timestamp": Utc.timestamp_opt(failed_at, 0).single().unwrap().to_rfc3339(),
        "type": "event_msg",
        "payload": {"type": "task_complete", "turn_id": "quota-turn", "last_agent_message": null,
            "error": {"message": "usage_limit_exceeded"}}
    });
    std::fs::write(rollout, format!("{event}\n")).unwrap();
    target.offset = Some(std::fs::metadata(rollout).unwrap().len());
}

#[test]
fn unselected_ownerless_retry_is_kept_undated_despite_a_stale_row() {
    // #19: SQLite `updated_at` cannot date an ownerless retry, so only the
    // selected target is dated, from its rollout. The unindexed-target
    // pre-pass must not grow a recency filter on the row.
    let home = rotation_home("unselected-stale-row");
    let [(first, first_rollout), (mut stale_row, stale_rollout)] =
        [1, 2].map(|index| ownerless(&home, index));
    let now = Utc::now().timestamp();
    let stale_id = stale_row.id.clone();
    let rows = |id: &str| Ok(Some(if id == stale_id { now - 5 * 3600 } else { now }));

    let rotation = rotation_at(&home, 0, 2);
    let scans = CheckpointScanRegistry::new(4);
    let mut targets = vec![first.clone(), stale_row.clone()];
    let mut tail_reads = Vec::new();
    ManifestPruneService::new(&rotation, &scans)
        .run_with_inspector(&home, &mut targets, rows, |_, id| {
            tail_reads.push(id.to_string());
            crate::switcher::ThreadRolloutState::ActiveInProgress
        })
        .unwrap();
    assert_eq!(targets, [first.clone(), stale_row.clone()]);
    assert_eq!(
        tail_reads,
        std::slice::from_ref(&first.id),
        "the unselected tail is unread"
    );
    assert_eq!(
        scanned(&scans, &[&first_rollout, &stale_rollout]),
        [true, false]
    );

    // Once selected, the stale-row target is dated from its failure time:
    // a recent failure keeps it and an old one drops it.
    for (failed_age, kept) in [(30, true), (5 * 3600, false)] {
        fail_on_quota(&stale_rollout, &mut stale_row, now - failed_age);
        let rotation = rotation_at(&home, 1, 2);
        let scans = CheckpointScanRegistry::new(4);
        let mut targets = vec![first.clone(), stale_row.clone()];
        ManifestPruneService::new(&rotation, &scans)
            .run_with(&home, &mut targets, rows)
            .unwrap();
        let expected = if kept {
            vec![first.clone(), stale_row.clone()]
        } else {
            vec![first.clone()]
        };
        assert_eq!(targets, expected, "failure {failed_age}s ago");
    }
    std::fs::remove_dir_all(home).unwrap();
}
