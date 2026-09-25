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
    // The mixed suffix is ambiguous after the final JSON value and is hidden
    // through the end, even when it contains independently recognizable data.
    assert!(!clean.contains("[PATH]"));
}

#[test]
fn independent_path_token_and_argument_markers_remain_available() {
    assert!(LogRedactionService::sanitize_text("/Users/dst/private").contains("[PATH]"));
    assert!(LogRedactionService::sanitize_text("Bearer abc").contains("[TOKEN]"));
    assert!(LogRedactionService::sanitize_text("--secret-flag").contains("[ARG]"));
}

#[test]
fn sanitizes_spaced_json_keyed_values_without_leaking_the_following_token() {
    let input = r#"{"token": "synthetic-secret", "display_name": "Synthetic Person", "email": "person@example.test"}"#;
    let clean = LogRedactionService::sanitize_text(input);

    assert!(!clean.contains("synthetic-secret"));
    assert!(!clean.contains("Synthetic Person"));
    assert!(!clean.contains("person@example.test"));
    assert!(clean.contains("[TOKEN]"));
    assert!(clean.contains("name_"));
    assert!(clean.contains("email_"));
}

#[test]
fn marker_prefix_does_not_make_a_secret_suffix_trusted() {
    let clean = LogRedactionService::sanitize_text("token=[TOKEN]synthetic-secret");

    assert_eq!(clean, "token=[TOKEN]");
    assert!(!clean.contains("synthetic-secret"));
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
    let operation = "op_dist_1720000000000_abcd1234ef56";
    assert_eq!(
        operation,
        LogRedactionService::sanitize_field("op_id", operation)
    );
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    assert!(LogRedactionService::sanitize_field("operation_id", uuid).starts_with("op_"));
}

#[test]
fn truncated_quoted_secret_does_not_escape_to_the_log() {
    let clean = LogRedactionService::sanitize_text("token=\"synthetic-secret without-close");
    assert!(!clean.contains("synthetic-secret"));
    assert!(!clean.contains("without-close"));
    assert!(clean.contains("[TOKEN]"));
}

#[test]
fn authorization_schemes_are_redacted_as_one_opaque_field() {
    for input in [
        "Authorization: Bearer synthetic-bearer-value",
        "Authorization: Basic synthetic-basic-value",
    ] {
        let clean = LogRedactionService::sanitize_text(input);
        assert!(!clean.contains("synthetic-bearer-value"));
        assert!(!clean.contains("synthetic-basic-value"));
        assert!(clean.contains("[TOKEN]"));
    }
}

#[test]
fn wrapped_credentials_and_all_list_identifiers_are_redacted() {
    let input = "(sk-synthetic-value) [tok_synthetic-value] first@example.test,second@example.test 550e8400-e29b-41d4-a716-446655440000,6ba7b810-9dad-41d1-80b4-00c04fd430c8";
    let clean = LogRedactionService::sanitize_text(input);
    for secret in [
        "sk-synthetic-value",
        "tok_synthetic-value",
        "first@example.test",
        "second@example.test",
        "550e8400-e29b-41d4-a716-446655440000",
        "6ba7b810-9dad-41d1-80b4-00c04fd430c8",
    ] {
        assert!(
            !clean.contains(secret),
            "unredacted synthetic value: {secret}"
        );
    }
}

#[test]
fn delimiter_joined_secret_and_malformed_quote_suffix_are_opaque() {
    for input in [
        "token=synthetic-first,synthetic-second",
        "api_key=synthetic-first;synthetic-second",
        "token=\"synthetic-first\"synthetic-tail",
    ] {
        let clean = LogRedactionService::sanitize_text(input);
        assert!(!clean.contains("synthetic-first"));
        assert!(!clean.contains("synthetic-second"));
        assert!(!clean.contains("synthetic-tail"));
    }
}

#[test]
fn comma_followed_by_a_key_without_delimiter_cannot_expose_a_secret_tail() {
    for input in [
        "token=\"synthetic-one\",password synthetic-two",
        "token=\"synthetic-one\", password synthetic-two",
        "token=\"synthetic-one\" ,password synthetic-two",
        "token=\"synthetic-one\" password synthetic-two",
        "token=\"synthetic-one\"}password synthetic-two",
        "{\"token\":\"synthetic-one\"} password synthetic-two",
        "token=\"synthetic-one\"\" synthetic-two",
        "token=\"synthetic-one\",password=\"synthetic-two\"",
    ] {
        let clean = LogRedactionService::sanitize_text(input);
        assert!(!clean.contains("synthetic-one"));
        assert!(
            !clean.contains("synthetic-two"),
            "malformed suffix leaked: {clean}"
        );
    }
}

#[test]
fn escaped_nested_json_secret_is_redacted() {
    let input = r#"{"message":"{\"token\":\"synthetic-secret\"}"}"#;
    let clean = LogRedactionService::sanitize_text(input);
    assert!(!clean.contains("synthetic-secret"));
    assert!(clean.contains("[TOKEN]"));
}

#[test]
fn escaped_quote_inside_a_secret_does_not_end_the_field() {
    let input = r#"token="synthetic-first\" synthetic-tail""#;
    let clean = LogRedactionService::sanitize_text(input);
    assert!(!clean.contains("synthetic-first"));
    assert!(!clean.contains("synthetic-tail"));
}

#[test]
fn twice_serialized_nested_json_secret_is_redacted() {
    let input = r#"{"message":"{\\\"token\\\":\\\"synthetic-secret\\\"}"}"#;
    let clean = LogRedactionService::sanitize_text(input);
    assert!(!clean.contains("synthetic-secret"));
}

#[test]
fn escaped_opening_quote_on_unquoted_key_hides_whole_value() {
    let input = r#"password=\"synthetic-first synthetic-tail\""#;
    let clean = LogRedactionService::sanitize_text(input);
    assert!(!clean.contains("synthetic-first"));
    assert!(!clean.contains("synthetic-tail"));
}

#[test]
fn unquoted_multiword_secret_and_name_hide_every_word() {
    for input in [
        "password: synthetic-first synthetic-tail",
        "display_name: Synthetic Person",
    ] {
        let clean = LogRedactionService::sanitize_text(input);
        assert!(!clean.contains("synthetic-tail"));
        assert!(!clean.contains("Person"));
    }
}

#[test]
fn trailing_backslash_in_a_sensitive_value_is_safe() {
    assert_eq!(
        LogRedactionService::sanitize_text("token=\\"),
        "token=[TOKEN]"
    );
}

#[test]
fn long_unstructured_token_without_email_is_handled() {
    let input = "a".repeat(1_048_576);
    assert_eq!(LogRedactionService::sanitize_text(&input), input);
}

#[test]
fn user_supplied_operation_prefix_is_not_trusted_as_generated() {
    let clean = LogRedactionService::sanitize_text("operation_id=op_dist_sensitive_text");
    assert!(!clean.contains("op_dist_sensitive_text"));
    assert!(clean.contains("op_"));
}

#[test]
fn nested_operational_fields_fail_closed_without_deep_recursion() {
    assert_eq!(
        LogRedactionService::sanitize_text("reason=token=synthetic-secret"),
        "reason=[TOKEN]"
    );
    let nested = format!("{}synthetic-tail", "reason=".repeat(10_000));
    let clean = LogRedactionService::sanitize_text(&nested);
    assert!(!clean.contains("synthetic-tail"));
}
