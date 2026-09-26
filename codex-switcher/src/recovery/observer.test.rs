use super::observer::{Observer, MAX_LINE};
use serde_json::Value;
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
fn event(kind: &str, payload: Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"type":kind,"payload":payload,"timestamp":"test"}))
        .unwrap()
}

#[test]
fn observer_ignores_old_work_and_handles_partial_and_huge_lines() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("cxi-observer-test-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let work = event("response_item", serde_json::json!({"type":"reasoning"}));
    file.write_all(&work).unwrap();
    file.write_all(b"\n").unwrap();
    let mut observer = Observer::checkpoint(path.clone()).unwrap();
    observer.poll().unwrap();
    assert!(!observer.evidence.verified(None));
    let started = event("event_msg", serde_json::json!({"type":"task_started"}));
    file.write_all(&started).unwrap();
    file.write_all(b"\n").unwrap();
    file.write_all(&vec![b'x'; MAX_LINE + 10]).unwrap();
    file.write_all(b"\n").unwrap();
    file.write_all(&work[..20]).unwrap();
    observer.poll().unwrap();
    assert!(!observer.evidence.verified(None));
    file.write_all(&work[20..]).unwrap();
    file.write_all(b"\n").unwrap();
    observer.poll().unwrap();
    assert!(observer.evidence.verified(None));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn observer_snapshot_does_not_chase_later_rollout_writes() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("cxi-observer-snapshot-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let mut observer = Observer::checkpoint(path.clone()).unwrap();
    file.write_all(&event(
        "event_msg",
        serde_json::json!({"type":"task_started"}),
    ))
    .unwrap();
    file.write_all(b"\n").unwrap();
    let snapshot = file.metadata().unwrap().len();
    file.write_all(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ))
    .unwrap();
    file.write_all(b"\n").unwrap();

    observer.poll_to(snapshot).unwrap();
    assert!(observer.evidence.started);
    assert!(!observer.evidence.work);
    observer.poll().unwrap();
    assert!(observer.evidence.verified(None));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn observer_rejects_replaced_rollout_after_checkpoint() {
    let home = std::env::temp_dir().join(format!("cxi-observer-replaced-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let path = home.join("rollout.jsonl");
    std::fs::write(&path, b"checkpoint\n").unwrap();
    let mut observer = Observer::checkpoint(path.clone()).unwrap();
    let replacement = home.join("replacement.jsonl");
    std::fs::write(&replacement, b"checkpoint\n").unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    let error = observer
        .poll_to(std::fs::metadata(&path).unwrap().len())
        .unwrap_err();
    assert!(error.contains("identity changed"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn newline_terminated_oversized_event_remains_untrusted() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "cxi-observer-complete-oversized-{}",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let mut observer = Observer::checkpoint(path.clone()).unwrap();
    let long_started = event(
        "event_msg",
        serde_json::json!({"type":"task_started","turn_id":"new-turn","padding":"x".repeat(MAX_LINE)}),
    );
    file.write_all(&long_started).unwrap();
    file.write_all(b"\n").unwrap();
    observer.poll().unwrap();
    assert!(
        observer.saw_oversized,
        "a skipped completed event must remain untrusted"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn newline_terminated_malformed_event_remains_untrusted() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("cxi-observer-malformed-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let mut observer = Observer::checkpoint(path.clone()).unwrap();
    file.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":\n")
        .unwrap();
    observer.poll().unwrap();
    assert!(
        observer.saw_malformed,
        "a skipped malformed event must remain untrusted"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn structurally_incomplete_json_record_remains_untrusted() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "cxi-observer-incomplete-object-{}",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let mut observer = Observer::checkpoint(path.clone()).unwrap();
    file.write_all(b"{}\n{\"type\":\"event_msg\",\"payload\":{}}\n")
        .unwrap();
    observer.poll().unwrap();
    assert!(
        observer.saw_malformed,
        "invalid record shape must not hide lifecycle state"
    );
    std::fs::remove_file(path).unwrap();
}
