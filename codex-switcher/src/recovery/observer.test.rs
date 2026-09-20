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
