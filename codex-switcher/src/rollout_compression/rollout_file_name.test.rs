use super::RolloutFileName;
use std::path::{Path, PathBuf};

const THREAD: &str = "019d4e9f-6555-79f1-84b6-758870d2c70a";
const ROLLOUT: &str = "01a081c6-c36c-77d2-b86a-d97119abbffb";

#[test]
fn parses_an_ordinary_rollout_name() {
    let name = format!("rollout-2026-04-03T00-36-02-{THREAD}.jsonl");
    let parsed = RolloutFileName::parse(&name).expect("canonical name");
    assert_eq!(parsed.thread_id(), THREAD);
}

#[test]
fn reverted_thread_name_keys_the_lock_by_the_first_id() {
    let name = format!("rollout-2026-09-09T02-08-02-{THREAD}_{ROLLOUT}.jsonl");
    let parsed = RolloutFileName::parse(&name).expect("reverted name");
    assert_eq!(parsed.thread_id(), THREAD);
}

#[test]
fn rejects_names_that_are_not_plain_canonical_rollouts() {
    for name in [
        format!("rollout-2026-04-03T00-36-02-{THREAD}.jsonl.zst"),
        format!(".rollout-2026-04-03T00-36-02-{THREAD}.jsonl.cxi-0123456789abcdef.tmp"),
        format!("rollout-compress-2026-04-03T00-36-02-{THREAD}.jsonl"),
        format!("rollout-2026-04-03 00-36-02-{THREAD}.jsonl"),
        format!("rollout-2026-04-03T00-36-02_{THREAD}.jsonl"),
        "rollout-2026-04-03T00-36-02-not-a-thread-id.jsonl".to_string(),
        format!("rollout-2026-04-03T00-36-02-{THREAD}_garbage.jsonl"),
        format!("session-2026-04-03T00-36-02-{THREAD}.jsonl"),
        "rollout-.jsonl".to_string(),
        String::new(),
    ] {
        assert!(RolloutFileName::parse(&name).is_none(), "accepted {name}");
    }
}

#[test]
fn compressed_and_plain_paths_round_trip() {
    let plain = PathBuf::from(format!(
        "/home/sessions/2026/04/03/rollout-2026-04-03T00-36-02-{THREAD}.jsonl"
    ));
    let compressed = RolloutFileName::compressed_path(&plain);
    assert_eq!(
        compressed.file_name().and_then(|n| n.to_str()),
        Some(format!("rollout-2026-04-03T00-36-02-{THREAD}.jsonl.zst").as_str())
    );
    assert_eq!(RolloutFileName::plain_path(&compressed), Some(plain));
}

#[test]
fn plain_path_rejects_non_rollout_compressed_files() {
    assert_eq!(
        RolloutFileName::plain_path(Path::new("/home/log.jsonl.zst")),
        None
    );
    assert_eq!(
        RolloutFileName::plain_path(Path::new(&format!(
            "/home/rollout-2026-04-03T00-36-02-{THREAD}.jsonl"
        ))),
        None
    );
}
