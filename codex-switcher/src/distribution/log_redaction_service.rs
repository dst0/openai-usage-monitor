const PATH_MARKER: &str = "[PATH]";
const TOKEN_MARKER: &str = "[TOKEN]";
const ARG_MARKER: &str = "[ARG]";
pub struct LogRedactionService;
impl LogRedactionService {
    pub fn sanitize_text(value: &str) -> String {
        let value = Self::remove_controls(value);
        let mut output = Vec::new();
        let mut redact_next = false;
        for token in value.split_whitespace() {
            if redact_next {
                output.push(TOKEN_MARKER.to_string());
                redact_next = false;
                continue;
            }
            let clean = Self::sanitize_token(token);
            redact_next = token.eq_ignore_ascii_case("bearer");
            output.push(clean);
        }
        output.join(" ")
    }

    pub fn sanitize_field(field: &str, value: &str) -> String {
        let field = field
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_');
        let field = field.to_ascii_lowercase();
        let value = value.trim();
        match field.as_str() {
            "email" | "user_email" => Self::opaque_ref("email", value),
            "account" | "account_id" | "account_ref" | "app_target" | "cli_target" => {
                Self::opaque_ref("acct", value)
            }
            "display_name" | "name" | "nickname" => Self::opaque_ref("name", value),
            "thread" | "thread_id" => Self::opaque_ref("thread", value),
            "turn" | "turn_id" => Self::opaque_ref("turn", value),
            "session" | "session_id" => Self::opaque_ref("session", value),
            "operation" | "operation_id" | "op" | "op_id" => Self::operation_ref(value),
            "path" | "file" | "filename" => PATH_MARKER.to_string(),
            "token" | "access_token" | "refresh_token" | "authorization" | "api_key"
            | "openai_api_key" | "secret" | "password" | "credential" => TOKEN_MARKER.to_string(),
            "arg" | "argv" | "args" | "flag" => ARG_MARKER.to_string(),
            _ => Self::sanitize_text(value),
        }
    }

    pub fn opaque_ref(namespace: &str, value: &str) -> String {
        let hash = Self::hash(namespace, value);
        format!("{namespace}_{hash:016x}")
    }

    pub fn operation_ref(value: &str) -> String {
        if value.starts_with("op_dist_")
            && value.len() <= 64
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            value.to_string()
        } else {
            Self::opaque_ref("op", value)
        }
    }

    fn sanitize_token(token: &str) -> String {
        if let Some((prefix, field, value, suffix)) = Self::keyed_value(token) {
            let suffix = if suffix.is_empty() {
                String::new()
            } else {
                Self::sanitize_text(suffix)
            };
            return format!("{prefix}{}{}", Self::sanitize_field(&field, value), suffix);
        }
        if let Some((start, end)) = Self::email_span(token) {
            return Self::replace_span(
                token,
                start,
                end,
                &Self::opaque_ref("email", &token[start..end]),
            );
        }
        if let Some((start, end)) = Self::path_span(token) {
            return Self::replace_span(token, start, end, PATH_MARKER);
        }
        if token.starts_with("--") || token.contains(" --") {
            return ARG_MARKER.to_string();
        }
        if token.starts_with("sk-")
            || token.starts_with("tok_")
            || token.starts_with("rt_")
            || Self::looks_like_jwt(token)
        {
            return TOKEN_MARKER.to_string();
        }
        Self::replace_uuid(token)
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
        if let Some(quote) = value
            .chars()
            .next()
            .filter(|character| *character == '\'' || *character == '"')
        {
            let close = value[quote.len_utf8()..].find(quote)? + quote.len_utf8();
            return Some((
                &token[..prefix_end + quote.len_utf8()],
                field,
                &value[quote.len_utf8()..close],
                &value[close + quote.len_utf8()..],
            ));
        }
        let end = value
            .char_indices()
            .find(|(_, character)| ",}]();".contains(*character))
            .map(|(index, _)| index)
            .unwrap_or(value.len());
        Some((&token[..prefix_end], field, &value[..end], &value[end..]))
    }

    fn email_span(token: &str) -> Option<(usize, usize)> {
        let bytes = token.as_bytes();
        for start in 0..bytes.len() {
            if !bytes[start].is_ascii_alphanumeric() {
                continue;
            }
            let mut at = start;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || b"._%+-".contains(&bytes[at]))
            {
                if bytes[at] == b'@' {
                    break;
                }
                at += 1;
            }
            if at >= bytes.len() || bytes[at] != b'@' || at == start {
                continue;
            }
            let mut end = at + 1;
            let mut dot = false;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || b".-".contains(&bytes[end]))
            {
                dot |= bytes[end] == b'.';
                end += 1;
            }
            if dot && end > at + 2 {
                return Some((start, end));
            }
        }
        None
    }

    fn path_span(token: &str) -> Option<(usize, usize)> {
        let bytes = token.as_bytes();
        for start in 0..bytes.len() {
            let slash =
                bytes[start] == b'/' && bytes.get(start + 1).is_some_and(|byte| *byte != b'/');
            let tilde = bytes[start] == b'~' && bytes.get(start + 1) == Some(&b'/');
            if !slash && !tilde {
                continue;
            }
            let mut end = start;
            while end < bytes.len() && !b" \t\"'`,;)]}".contains(&bytes[end]) {
                end += 1;
            }
            if end > start + 1 {
                return Some((start, end));
            }
        }
        None
    }

    fn looks_like_jwt(token: &str) -> bool {
        token.starts_with("eyJ") && token.matches('.').count() >= 2
    }

    fn replace_uuid(token: &str) -> String {
        let bytes = token.as_bytes();
        if bytes.len() < 36 {
            return token.to_string();
        }
        for start in 0..=bytes.len() - 36 {
            let candidate = &bytes[start..start + 36];
            let valid = candidate.iter().enumerate().all(|(index, byte)| {
                if [8, 13, 18, 23].contains(&index) {
                    *byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            });
            if valid {
                let value = String::from_utf8_lossy(candidate);
                return Self::replace_span(
                    token,
                    start,
                    start + 36,
                    &Self::opaque_ref("id", &value),
                );
            }
        }
        token.to_string()
    }

    fn replace_span(value: &str, start: usize, end: usize, replacement: &str) -> String {
        format!("{}{}{}", &value[..start], replacement, &value[end..])
    }

    fn remove_controls(value: &str) -> String {
        let mut output = String::new();
        let mut chars = value.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    while let Some(next) = chars.next() {
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

    fn hash(namespace: &str, value: &str) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in namespace
            .bytes()
            .chain([0].into_iter())
            .chain(value.as_bytes().iter().copied())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}
