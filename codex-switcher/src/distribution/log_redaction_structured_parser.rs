use super::log_redaction_service::LogRedactionService;

pub(super) fn sanitize_structured_values(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut copied_until = 0;
    let mut cursor = 0;

    while cursor < input.len() {
        let Some((field, after_key)) = parse_sensitive_key(input, cursor) else {
            cursor += input[cursor..].chars().next().unwrap().len_utf8();
            continue;
        };
        let Some((value_start, value_end)) = parse_value(input, after_key) else {
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

fn parse_sensitive_key(input: &str, start: usize) -> Option<(&str, usize)> {
    if start > 0 {
        let previous = input[..start].chars().next_back()?;
        if previous.is_ascii_alphanumeric() || previous == '_' {
            return None;
        }
    }
    let first = input[start..].chars().next()?;
    let (field_start, field_end, after_key) = if matches!(first, '\'' | '"') {
        let field_start = start + first.len_utf8();
        let relative_end = input[field_start..].find(first)?;
        let field_end = field_start + relative_end;
        (field_start, field_end, field_end + first.len_utf8())
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
    let field = &input[field_start..field_end];
    if !is_sensitive_field(field) {
        return None;
    }
    Some((field, after_key))
}

fn parse_value(input: &str, after_key: usize) -> Option<(usize, usize)> {
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
    if matches!(first, '\'' | '"') {
        value_start += first.len_utf8();
        return quoted_value_end(input, value_start, first).map(|end| (value_start, end));
    }
    let value_end = input[value_start..]
        .char_indices()
        .find(|(_, character)| character.is_whitespace() || ",}();".contains(*character))
        .map(|(index, _)| value_start + index)
        .unwrap_or(input.len());
    (value_end > value_start).then_some((value_start, value_end))
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
