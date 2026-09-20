use super::thread_identity::valid_id;

#[test]
fn rejects_sql_or_url_injection() {
    assert!(valid_id("01a098c2-0fae-74d2-a80c-45d89e910e79"));
    assert!(!valid_id("' OR 1=1;--"));
    assert!(!valid_id(
        "codex://threads/01a098c2-0fae-74d2-a80c-45d89e910e79"
    ));
}
