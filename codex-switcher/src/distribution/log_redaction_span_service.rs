use super::log_redaction_service::LogRedactionService;
use super::log_redaction_token_service::PATH_MARKER;

pub(super) struct LogRedactionSpanService;

impl LogRedactionSpanService {
    pub(super) fn redact_emails(token: &str) -> String {
        if !token.contains('@') {
            return token.to_string();
        }
        Self::replace_all(token, Self::email_span, |value| {
            LogRedactionService::opaque_ref("email", value)
        })
    }

    pub(super) fn redact_paths(token: &str) -> String {
        if !token.contains('/') && !token.contains('~') {
            return token.to_string();
        }
        Self::replace_all(token, Self::path_span, |_| PATH_MARKER.to_string())
    }

    pub(super) fn redact_uuids(token: &str) -> String {
        if !token.contains('-') {
            return token.to_string();
        }
        Self::replace_all(token, Self::uuid_span, |value| {
            LogRedactionService::opaque_ref("id", value)
        })
    }

    fn replace_all<F, R>(token: &str, mut next: F, mut replacement: R) -> String
    where
        F: FnMut(&str, usize) -> Option<(usize, usize)>,
        R: FnMut(&str) -> String,
    {
        let mut output = String::with_capacity(token.len());
        let mut cursor = 0;
        while let Some((start, end)) = next(token, cursor) {
            output.push_str(&token[cursor..start]);
            output.push_str(&replacement(&token[start..end]));
            cursor = end;
        }
        output.push_str(&token[cursor..]);
        output
    }

    fn email_span(token: &str, from: usize) -> Option<(usize, usize)> {
        let bytes = token.as_bytes();
        let mut cursor = from;
        while cursor < bytes.len() {
            if !bytes[cursor].is_ascii_alphanumeric() {
                cursor += 1;
                continue;
            }
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || b"._%+-".contains(&bytes[cursor]))
            {
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&b'@') {
                continue;
            }
            let at = cursor;
            cursor += 1;
            let mut dot = false;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || b".-".contains(&bytes[cursor]))
            {
                dot |= bytes[cursor] == b'.';
                cursor += 1;
            }
            if dot && cursor > at + 2 {
                return Some((start, cursor));
            }
        }
        None
    }

    fn path_span(token: &str, from: usize) -> Option<(usize, usize)> {
        let bytes = token.as_bytes();
        let mut cursor = from;
        while cursor < bytes.len() {
            let slash =
                bytes[cursor] == b'/' && bytes.get(cursor + 1).is_some_and(|byte| *byte != b'/');
            let tilde = bytes[cursor] == b'~' && bytes.get(cursor + 1) == Some(&b'/');
            if !slash && !tilde {
                cursor += 1;
                continue;
            }
            let start = cursor;
            while cursor < bytes.len() && !b" \t\"'`,;)]}".contains(&bytes[cursor]) {
                cursor += 1;
            }
            if cursor > start + 1 {
                return Some((start, cursor));
            }
            cursor += 1;
        }
        None
    }

    fn uuid_span(token: &str, from: usize) -> Option<(usize, usize)> {
        let bytes = token.as_bytes();
        if bytes.len() < 36 || from > bytes.len() - 36 {
            return None;
        }
        for start in from..=bytes.len() - 36 {
            let candidate = &bytes[start..start + 36];
            let valid = candidate.iter().enumerate().all(|(index, byte)| {
                if [8, 13, 18, 23].contains(&index) {
                    *byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            });
            if valid {
                return Some((start, start + 36));
            }
        }
        None
    }
}
