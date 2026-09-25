use super::log_redaction_service::LogRedactionService;
use super::log_redaction_span_service::LogRedactionSpanService;

pub(crate) const PATH_MARKER: &str = "[PATH]";
pub(crate) const TOKEN_MARKER: &str = "[TOKEN]";
pub(crate) const ARG_MARKER: &str = "[ARG]";

pub(crate) struct LogRedactionTokenService;

impl LogRedactionTokenService {
    pub(crate) fn sanitize_token(token: &str) -> String {
        if !matches!(token.chars().next(), Some('{' | '[' | '"' | '\'')) {
            if let Some((prefix, field, value, suffix)) = Self::keyed_value(token) {
                return format!(
                    "{prefix}{}{suffix}",
                    LogRedactionService::sanitize_field(&field, value)
                );
            }
        }
        if token.starts_with("--") || token.contains(" --") {
            return ARG_MARKER.to_string();
        }
        if Self::contains_credential(token) {
            return TOKEN_MARKER.to_string();
        }
        let clean = LogRedactionSpanService::redact_emails(token);
        let clean = LogRedactionSpanService::redact_paths(&clean);
        LogRedactionSpanService::redact_uuids(&clean)
    }

    pub(crate) fn remove_controls(value: &str) -> String {
        let mut output = String::new();
        let mut chars = value.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for next in chars.by_ref() {
                        if next.is_ascii_alphabetic() || next == '@' {
                            break;
                        }
                    }
                }
                output.push(' ');
            } else if character.is_control() {
                output.push(' ');
            } else {
                output.push(character);
            }
        }
        output
    }

    fn keyed_value(token: &str) -> Option<(&str, String, &str, &str)> {
        let (index, separator) = token.char_indices().find_map(|(index, character)| {
            (character == '=' || character == ':').then_some((index, character))
        })?;
        let raw_key = &token[..index];
        let field = raw_key
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .to_ascii_lowercase();
        let known = matches!(
            field.as_str(),
            "email"
                | "user_email"
                | "account"
                | "account_id"
                | "account_ref"
                | "app_target"
                | "cli_target"
                | "display_name"
                | "name"
                | "nickname"
                | "thread"
                | "thread_id"
                | "turn"
                | "turn_id"
                | "session"
                | "session_id"
                | "operation"
                | "operation_id"
                | "op"
                | "op_id"
                | "path"
                | "file"
                | "filename"
                | "token"
                | "access_token"
                | "refresh_token"
                | "authorization"
                | "api_key"
                | "openai_api_key"
                | "secret"
                | "password"
                | "credential"
                | "arg"
                | "argv"
                | "args"
                | "flag"
                | "reason"
                | "trigger"
                | "phase"
                | "status"
                | "outcome"
                | "transport"
                | "detail"
        );
        if !known {
            return None;
        }
        let prefix_end = index + separator.len_utf8();
        let value = &token[prefix_end..];
        for marker in [PATH_MARKER, TOKEN_MARKER, ARG_MARKER] {
            if let Some(suffix) = value.strip_prefix(marker) {
                if suffix.chars().all(|character| ",}]();".contains(character)) {
                    return Some((&token[..prefix_end], field, marker, suffix));
                }
            }
        }
        if let Some(quote) = value
            .chars()
            .next()
            .filter(|character| *character == '\'' || *character == '"')
        {
            let Some(close) = value[quote.len_utf8()..].find(quote) else {
                return Some((
                    &token[..prefix_end + quote.len_utf8()],
                    field,
                    &value[quote.len_utf8()..],
                    "",
                ));
            };
            let close = close + quote.len_utf8();
            let suffix = &value[close + quote.len_utf8()..];
            let suffix = if suffix.chars().all(|character| ",}]();".contains(character)) {
                suffix
            } else {
                ""
            };
            return Some((
                &token[..prefix_end + quote.len_utf8()],
                field,
                &value[quote.len_utf8()..close],
                suffix,
            ));
        }
        Some((&token[..prefix_end], field, value, ""))
    }

    fn contains_credential(token: &str) -> bool {
        if !["sk-", "tok_", "rt_", "eyJ"]
            .iter()
            .any(|prefix| token.contains(prefix))
        {
            return false;
        }
        let mut dots_remaining = token.bytes().filter(|byte| *byte == b'.').count();
        for (index, character) in token.char_indices() {
            if character == '.' {
                dots_remaining -= 1;
            }
            if index > 0
                && token[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                continue;
            }
            let candidate = &token[index..];
            if ["sk-", "tok_", "rt_"]
                .iter()
                .any(|prefix| candidate.starts_with(prefix))
                || (candidate.starts_with("eyJ") && dots_remaining >= 2)
            {
                return true;
            }
        }
        false
    }
}
