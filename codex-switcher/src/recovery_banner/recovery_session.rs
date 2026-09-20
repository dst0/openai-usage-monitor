use super::banner_session_status::BannerSessionStatus;
use serde::{Deserialize, Serialize};

const MAX_PROJECT_CHARS: usize = 96;
const MAX_TITLE_CHARS: usize = 160;
const MAX_SHORT_ID_CHARS: usize = 12;

/// Sanitized user-facing identity for one recovery target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverySession {
    pub project: String,
    pub title: String,
    pub short_id: String,
    pub status: BannerSessionStatus,
}

impl RecoverySession {
    pub fn from_raw(
        project: impl AsRef<str>,
        title: impl AsRef<str>,
        session_id: impl AsRef<str>,
        status: BannerSessionStatus,
    ) -> Self {
        Self {
            project: project_label(project.as_ref()),
            title: safe_text(title.as_ref(), "Без названия", MAX_TITLE_CHARS, true),
            short_id: short_id(session_id.as_ref()),
            status,
        }
    }

    pub fn display_line(&self) -> String {
        format!("{} / {} · {}", self.project, self.title, self.short_id)
    }
}

fn project_label(raw: &str) -> String {
    let normalized = raw.trim().replace('\\', "/");
    let last = normalized
        .split('/')
        .rev()
        .find(|part| !part.trim().is_empty())
        .unwrap_or("");
    safe_text(last, "Без проекта", MAX_PROJECT_CHARS, false)
}

fn short_id(raw: &str) -> String {
    let filtered: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    let value: String = filtered.chars().take(MAX_SHORT_ID_CHARS).collect();
    if value.is_empty() {
        "без-id".to_string()
    } else {
        value
    }
}

fn safe_text(raw: &str, fallback: &str, limit: usize, replace_slashes: bool) -> String {
    let mut output = String::new();
    let mut previous_was_space = false;
    for ch in raw.trim().chars() {
        let ch = if replace_slashes && (ch == '/' || ch == '\\') {
            '／'
        } else {
            ch
        };
        if ch.is_control() && !ch.is_whitespace() {
            continue;
        }
        if ch.is_whitespace() {
            if previous_was_space {
                continue;
            }
            previous_was_space = true;
            output.push(' ');
        } else {
            previous_was_space = false;
            output.push(ch);
        }
        if output.chars().count() >= limit {
            break;
        }
    }
    let mut output: String = output.trim().chars().take(limit).collect();
    redact_long_identifier(&mut output);
    if output.is_empty() {
        fallback.to_string()
    } else {
        output
    }
}

fn redact_long_identifier(value: &mut String) {
    let mut redacted = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, redacted: &mut String| {
        if token.chars().count() >= 24
            && token.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        {
            redacted.extend(token.chars().take(8));
            redacted.push('…');
        } else {
            redacted.push_str(token);
        }
        token.clear();
    };
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            token.push(ch);
        } else {
            flush(&mut token, &mut redacted);
            redacted.push(ch);
        }
    }
    flush(&mut token, &mut redacted);
    *value = redacted;
}
