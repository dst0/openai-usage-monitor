use super::log_redaction_service::LogRedactionService;

#[test]
fn redacts_identity_values_with_stable_namespaces() {
    let email = LogRedactionService::sanitize_field("email", "quoted.user+tag@example.test");
    assert_eq!(
        email,
        LogRedactionService::sanitize_field("email", "quoted.user+tag@example.test")
    );
    assert!(email.starts_with("email_"));
    assert_ne!(
        email,
        LogRedactionService::sanitize_field("account_id", "quoted.user+tag@example.test")
    );
    assert_eq!(
        "thread_",
        &LogRedactionService::sanitize_field("thread_id", "550e8400-e29b-41d4-a716-446655440000")
            [..7]
    );
    assert!(
        LogRedactionService::sanitize_field("display_name", "Юлия Пример").starts_with("name_")
    );
}

#[test]
fn sanitizes_json_quoted_values_paths_tokens_args_and_controls() {
    let input = "{\"email\":\"quoted.user@example.test\",\"display_name\":\"Юлия\"} /Users/dst/private --secret-flag Bearer abc sk-live-secret eyJheader.payload.signature\r\n\u{1b}[31mred\u{1b}[0m";
    let clean = LogRedactionService::sanitize_text(input);
    assert!(!clean.contains("quoted.user@example.test"));
    assert!(!clean.contains("Юлия"));
    assert!(!clean.contains("/Users/dst/private"));
    assert!(!clean.contains("secret-flag"));
    assert!(!clean.contains("abc"));
    assert!(!clean.contains("sk-live-secret"));
    assert!(!clean.contains("eyJheader"));
    assert!(!clean.contains('\r'));
    assert!(!clean.contains('\n'));
    assert!(!clean.contains('\u{1b}'));
    assert!(clean.contains("[PATH]"));
    assert!(clean.contains("[TOKEN]"));
    assert!(clean.contains("[ARG]"));
}

#[test]
fn preserves_operational_reason_and_sanitizes_keyed_ids() {
    let clean = LogRedactionService::sanitize_text(
        "reason=quota_exhausted thread=550e8400-e29b-41d4-a716-446655440000 turn=6ba7b810-9dad-41d1-80b4-00c04fd430c8 status=failed",
    );
    assert!(clean.contains("reason=quota_exhausted"));
    assert!(clean.contains("status=failed"));
    assert!(clean.contains("thread=thread_"));
    assert!(clean.contains("turn=turn_"));
    assert!(!clean.contains("550e8400-e29b-41d4-a716-446655440000"));
}

#[test]
fn generated_distribution_operation_ids_keep_correlation_contract() {
    let operation = "op_dist_1720000000000_abcd1234";
    assert_eq!(
        operation,
        LogRedactionService::sanitize_field("op_id", operation)
    );
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    assert!(LogRedactionService::sanitize_field("operation_id", uuid).starts_with("op_"));
}
