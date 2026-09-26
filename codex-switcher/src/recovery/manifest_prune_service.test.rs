use super::{ManifestPruneService, OwnerlessProbeRotation, PendingTarget};
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
            let result = ManifestPruneService::new(&rotation)
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
        let result = ManifestPruneService::new(&rotation).run_with_inspector(
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
