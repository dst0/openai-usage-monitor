use super::queue_snapshot::{
    parse_queued_rows, prepare_interrupted_queue, validate_queue_snapshot_revision,
    INTERRUPTED_QUEUE_PAUSE,
};
use serde_json::Value;

#[test]
fn queue_recovery_strips_only_the_restart_pause() {
    let mut messages = vec![
        serde_json::json!({
            "id": "one",
            "pausedReason": INTERRUPTED_QUEUE_PAUSE,
            "context": { "keep": true }
        }),
        serde_json::json!({ "id": "two", "context": { "keep": true } }),
    ];
    assert!(prepare_interrupted_queue(&mut messages).unwrap());
    assert!(messages[0].get("pausedReason").is_none());
    assert_eq!(messages[0]["context"]["keep"], Value::Bool(true));
    assert_eq!(messages[1]["id"].as_str(), Some("two"));

    let mut manually_paused = vec![serde_json::json!({
        "id": "manual",
        "pausedReason": "Paused by the user"
    })];
    assert!(prepare_interrupted_queue(&mut manually_paused).is_err());
    assert_eq!(
        manually_paused[0]["pausedReason"].as_str(),
        Some("Paused by the user")
    );
}

#[test]
fn queue_snapshot_requires_an_unchanged_revision() {
    assert!(validate_queue_snapshot_revision(41, 41).is_ok());
    assert!(validate_queue_snapshot_revision(41, 42).is_err());
}

#[test]
fn empty_sqlite_json_output_is_an_empty_queue() {
    assert_eq!(parse_queued_rows(b"").unwrap(), Vec::<Value>::new());
    assert_eq!(parse_queued_rows(b"\n \t").unwrap(), Vec::<Value>::new());
    assert!(parse_queued_rows(b"not-json").is_err());
}
