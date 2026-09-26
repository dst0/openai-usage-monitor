use super::{
    foreground_checkpoint_service::ForegroundCheckpointService,
    observer::Observer,
    recovery_mode::RecoveryMode,
    recovery_target::{RecoveryTarget, FOREGROUND_SCAN_BUDGET_BYTES},
    target_dispatch::revalidate_after_owner_with_budget,
};
use crate::switcher::ThreadRolloutState;
use std::{
    cell::Cell,
    fs::File,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

fn candidate(home: &Path, suffix: usize, bytes_after_checkpoint: u64) -> RecoveryTarget {
    let path = home.join(format!("rollout-{suffix}.jsonl"));
    let mut file = File::create(&path).unwrap();
    file.write_all(b"checkpoint\n").unwrap();
    let checkpoint = file.metadata().unwrap().len();
    if bytes_after_checkpoint > super::observer::MAX_LINE as u64 {
        let record = format!(
            "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
            "x".repeat(8 * 1024)
        );
        for _ in 0..(bytes_after_checkpoint as usize).div_ceil(record.len()) {
            file.write_all(record.as_bytes()).unwrap();
        }
    } else {
        file.set_len(checkpoint + bytes_after_checkpoint).unwrap();
    }
    RecoveryTarget {
        id: format!("01a098c2-0fae-74d2-a80c-{suffix:012x}"),
        state: ThreadRolloutState::ActiveInProgress,
        writer_locked: false,
        observer: Observer::checkpoint_at(path, checkpoint).unwrap(),
        scan_complete: false,
        existing_queue: 0,
        mounted_by_recovery: false,
        owner_unavailable: false,
        account_mismatch: false,
        dispatched: false,
        completed: false,
        failure: None,
        deadline: Instant::now(),
        execution_deadline_set: false,
        expected_turn_id: None,
        proof_observed_at: None,
    }
}

#[test]
fn two_foreground_targets_share_one_scan_budget_and_timeout_without_dispatch() {
    let home = std::env::temp_dir().join(format!("codex-foreground-fair-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut targets = vec![
        candidate(&home, 1, 40 * 1024 * 1024),
        candidate(&home, 2, 40 * 1024 * 1024),
    ];
    let initial_offsets = targets
        .iter()
        .map(|target| target.observer.offset)
        .collect::<Vec<_>>();
    let start = Instant::now();
    let clock = Cell::new(start);
    let waits = Cell::new(0);
    let skipped = ForegroundCheckpointService::new(&mut targets).scan_until_ready_with(
        || clock.get(),
        |_, current| {
            if waits.get() == 0 {
                let scanned = current
                    .iter()
                    .zip(&initial_offsets)
                    .map(|(target, initial)| target.observer.offset - initial)
                    .collect::<Vec<_>>();
                assert!(scanned.iter().all(|bytes| *bytes > 0));
                assert!(scanned.iter().sum::<u64>() <= FOREGROUND_SCAN_BUDGET_BYTES);
                assert!(current
                    .iter()
                    .all(|target| !target.dispatched && !target.scan_complete));
                clock.set(start + Duration::from_secs(91));
            }
            waits.set(waits.get() + 1);
        },
    );
    assert!(skipped.is_empty());
    assert!(targets
        .iter()
        .all(|target| target.owner_unavailable && target.failure.is_some()));
    assert!(targets
        .iter()
        .all(|target| !target.dispatched && !target.completed));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn rollout_replacement_during_foreground_scan_fails_closed() {
    let home =
        std::env::temp_dir().join(format!("codex-foreground-replace-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut targets = vec![candidate(&home, 3, 1)];
    let start = Instant::now();
    let clock = Cell::new(start);
    let mut replaced = false;
    ForegroundCheckpointService::new(&mut targets).scan_until_ready_with(
        || clock.get(),
        |_, current| {
            if !replaced {
                let path = &current[0].observer.path;
                let replacement = home.join("replacement.jsonl");
                std::fs::write(&replacement, b"checkpoint\n\n").unwrap();
                std::fs::rename(&replacement, path).unwrap();
                replaced = true;
            }
            clock.set(start + Duration::from_secs(4));
        },
    );
    assert!(targets[0].owner_unavailable);
    assert!(!targets[0].dispatched);
    assert!(targets[0]
        .failure
        .as_deref()
        .unwrap()
        .contains("identity changed"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn completed_scans_reject_late_growth_before_dispatch_without_extra_io() {
    let home =
        std::env::temp_dir().join(format!("codex-foreground-complete-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut targets = vec![
        candidate(&home, 4, 20 * 1024 * 1024),
        candidate(&home, 5, 20 * 1024 * 1024),
    ];
    for target in &targets {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&target.observer.path)
            .unwrap();
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::End(-1)).unwrap();
        file.write_all(b"\n").unwrap();
    }
    for target in &mut targets {
        let checkpoint = target.observer.offset;
        target.observer =
            Observer::checkpoint_at(target.observer.path.clone(), checkpoint).unwrap();
    }
    let start = Instant::now();
    let clock = Cell::new(start);
    let previous = std::cell::RefCell::new(
        targets
            .iter()
            .map(|target| target.observer.offset)
            .collect::<Vec<_>>(),
    );
    ForegroundCheckpointService::new(&mut targets).scan_until_ready_with(
        || clock.get(),
        |_, current| {
            let mut offsets = previous.borrow_mut();
            let scanned = current
                .iter()
                .zip(offsets.iter_mut())
                .map(|(target, offset)| {
                    let delta = target.observer.offset - *offset;
                    *offset = target.observer.offset;
                    delta
                })
                .sum::<u64>();
            assert!(scanned <= FOREGROUND_SCAN_BUDGET_BYTES);
            clock.set(clock.get() + Duration::from_secs(1));
        },
    );
    assert!(targets
        .iter()
        .all(|target| target.scan_complete && target.failure.is_none()));

    let mut budget = FOREGROUND_SCAN_BUDGET_BYTES / targets.len() as u64;
    let original_budget = budget;
    let late = &mut targets[0];
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&late.observer.path)
        .unwrap();
    writer.write_all(&vec![b'x'; 9 * 1024 * 1024]).unwrap();
    writer.write_all(b"\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n").unwrap();
    let result = revalidate_after_owner_with_budget(
        &home,
        late,
        0,
        0,
        RecoveryMode::DeferredCaptured,
        &mut budget,
    );
    assert!(result.unwrap_err().contains("Rollout identity changed"));
    assert_eq!(budget, original_budget);
    assert!(late.scan_complete && !late.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn rollout_truncation_during_foreground_scan_retains_original_checkpoint() {
    let home =
        std::env::temp_dir().join(format!("codex-foreground-truncate-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut targets = vec![candidate(&home, 6, 1)];
    let start = Instant::now();
    let clock = Cell::new(start);
    let mut truncated = false;
    ForegroundCheckpointService::new(&mut targets).scan_until_ready_with(
        || clock.get(),
        |_, current| {
            if !truncated {
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(&current[0].observer.path)
                    .unwrap()
                    .set_len(0)
                    .unwrap();
                truncated = true;
            }
            clock.set(start + Duration::from_secs(4));
        },
    );
    assert!(targets[0].owner_unavailable);
    assert!(!targets[0].dispatched);
    assert!(targets[0]
        .failure
        .as_deref()
        .unwrap()
        .contains("identity changed"));
    std::fs::remove_dir_all(home).unwrap();
}
