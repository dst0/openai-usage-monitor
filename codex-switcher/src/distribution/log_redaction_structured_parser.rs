use super::log_redaction_service::LogRedactionService;
#[path = "log_redaction_structured_suffix.rs"]
mod structured_suffix;
use std::collections::HashSet;
use structured_suffix::{quoted_suffix_is_structural, SuffixValidation};

pub(super) fn sanitize_structured_values(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut copied_until = 0;
    let mut cursor = 0;
    let mut suffix = SuffixValidation {
        trusted: HashSet::new(),
        remaining: input.len().saturating_mul(4),
    };

    while cursor < input.len() {
        let Some((field, after_key, escaped_key)) = parse_sensitive_key(input, cursor) else {
            cursor += input[cursor..]
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .count()
                .max(input[cursor..].chars().next().unwrap().len_utf8());
            continue;
        };
        let Some((value_start, value_end)) =
            parse_value(input, field, after_key, escaped_key, &mut suffix)
        else {
            cursor += input[cursor..].chars().next().unwrap().len_utf8();
            continue;
        };
        output.push_str(&input[copied_until..value_start]);
        output.push_str(&LogRedactionService::sanitize_field(
            field,
            &input[value_start..value_end],
        ));
        copied_until = value_end;
        cursor = value_end.max(cursor + 1);
    }

    if copied_until == 0 {
        input.to_string()
    } else {
        output.push_str(&input[copied_until..]);
        output
    }
}

fn parse_sensitive_key(input: &str, start: usize) -> Option<(&str, usize, bool)> {
    let (field, after_key, escaped_key) = parse_key(input, start)?;
    is_sensitive_field(field).then_some((field, after_key, escaped_key))
}

fn parse_key(input: &str, start: usize) -> Option<(&str, usize, bool)> {
    if start > 0 {
        let previous = input[..start].chars().next_back()?;
        if previous.is_ascii_alphanumeric() || previous == '_' {
            return None;
        }
    }
    let first = input[start..].chars().next()?;
    let escaped_key = start > 0 && input.as_bytes()[start - 1] == b'\\';
    let (field_start, field_end, after_key) = if matches!(first, '\'' | '"') {
        let field_start = start + first.len_utf8();
        if escaped_key {
            let terminator = format!("\\{first}");
            let field_end = field_start + input[field_start..].find(&terminator)?;
            (field_start, field_end, field_end + terminator.len())
        } else {
            let field_end = quoted_value_end(input, field_start, first)?;
            (field_start, field_end, field_end + first.len_utf8())
        }
    } else if first.is_ascii_alphanumeric() || first == '_' {
        let field_end = input[start..]
            .char_indices()
            .find(|(_, character)| !character.is_ascii_alphanumeric() && *character != '_')
            .map(|(index, _)| start + index)
            .unwrap_or(input.len());
        (start, field_end, field_end)
    } else {
        return None;
    };
    let field = if escaped_key {
        input[field_start..field_end].trim_end_matches('\\')
    } else {
        &input[field_start..field_end]
    };
    if field.is_empty() {
        return None;
    }
    Some((field, after_key, escaped_key))
}

fn parse_value(
    input: &str,
    field: &str,
    after_key: usize,
    escaped_key: bool,
    suffix: &mut SuffixValidation,
) -> Option<(usize, usize)> {
    let mut separator = after_key;
    while let Some(character) = input[separator..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        separator += character.len_utf8();
    }
    let delimiter = input[separator..].chars().next()?;
    if !matches!(delimiter, ':' | '=') {
        return None;
    }
    let mut value_start = separator + delimiter.len_utf8();
    while let Some(character) = input[value_start..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        value_start += character.len_utf8();
    }
    let first = input[value_start..].chars().next()?;
    if escaped_key {
        // A serialized JSON key can contain further escaped delimiters and
        // whitespace in its value. Preserve the key, but hide the whole tail.
        return Some((value_start, input.len()));
    }
    if first == '\\'
        && value_start
            .checked_add(1)
            .and_then(|next| input.get(next..))
            .is_some_and(|tail| tail.starts_with('"') || tail.starts_with('\''))
    {
        // The opening quote is escaped in a serialized diagnostic. Its
        // closing boundary is ambiguous, so hide the entire remaining tail.
        return Some((value_start, input.len()));
    }
    if matches!(first, '\'' | '"') {
        value_start += first.len_utf8();
        let Some(value_end) = quoted_value_end(input, value_start, first) else {
            return Some((value_start, input.len()));
        };
        let after_quote = value_end + first.len_utf8();
        return Some((
            value_start,
            if quoted_suffix_is_structural(input, after_quote, suffix) {
                value_end
            } else {
                input.len()
            },
        ));
    }
    let value_end = input[value_start..]
        .char_indices()
        .find(|(_, character)| character.is_whitespace())
        .map(|(index, _)| value_start + index)
        .unwrap_or(input.len());
    // Only fixed-format operation identifiers have an unambiguous unquoted
    // whitespace boundary. Names, paths, passphrases and headers may span words.
    let fixed_id_field = ["thread", "thread_id", "turn", "turn_id"]
        .iter()
        .any(|key| field.eq_ignore_ascii_case(key));
    if fixed_id_field && looks_like_uuid(&input[value_start..value_end]) {
        Some((value_start, value_end))
    } else {
        Some((value_start, input.len()))
    }
}

fn looks_like_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn quoted_value_end(input: &str, start: usize, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (offset, character) in input[start..].char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return Some(start + offset);
        }
    }
    None
}

fn is_sensitive_field(field: &str) -> bool {
    matches!(
        field.to_ascii_lowercase().as_str(),
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
    )
}
