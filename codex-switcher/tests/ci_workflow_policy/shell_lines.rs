//! Logical command lines of a workflow or shell script for `locked_cargo.rs`.
//! A trailing `\` outside single quotes joins the next line, as the shell
//! does. `#` starts a comment only outside quotes, at the start of a line or
//! after a space or tab. Quote state carries across lines, because a quoted
//! string (an `osascript` or `awk` program) may span several; a quote still
//! open at the end of the text makes the rest unreadable and is reported.
//! Lines are not joined across an open quote, so a flag on a later line can
//! never count toward a command on an earlier one. A YAML `name:` label that
//! starts a line outside any quote or continuation never runs and is skipped.

use crate::yaml_lines::entry;

/// Command lines of a text, and where an unterminated quote opened.
#[derive(Debug, PartialEq)]
pub struct ShellLines {
    /// `(1-based first line, text)` with comments removed and `\`
    /// continuations joined.
    pub lines: Vec<(usize, String)>,
    /// 1-based line of a quote that is never closed.
    pub open_quote_line: Option<usize>,
}

impl ShellLines {
    pub fn read(text: &str) -> Self {
        let mut lines = Vec::new();
        let mut pending: Option<(usize, String)> = None;
        let mut quote: Option<(char, usize)> = None;
        for (i, raw) in text.lines().enumerate() {
            let continuing = pending.is_some() || quote.is_some();
            if !continuing && entry(raw).is_some_and(|e| e.key == "name") {
                continue;
            }
            let (code, continued) = strip_comment(raw, &mut quote, i + 1);
            let (start, mut joined) = pending.take().unwrap_or((i + 1, String::new()));
            joined.push(' ');
            joined.push_str(code);
            if continued {
                pending = Some((start, joined));
            } else if !joined.trim().is_empty() {
                lines.push((start, joined));
            }
        }
        lines.extend(pending);
        Self {
            lines,
            open_quote_line: quote.map(|(_, line)| line),
        }
    }
}

/// `raw` before any comment, without a continuation backslash, and whether
/// that backslash continues the command on the next line. Updates `quote`,
/// the open quote character and the line it opened on.
fn strip_comment<'a>(
    raw: &'a str,
    quote: &mut Option<(char, usize)>,
    line: usize,
) -> (&'a str, bool) {
    let mut chars = raw.char_indices().peekable();
    let mut previous = None;
    while let Some((at, c)) = chars.next() {
        match (quote.map(|(q, _)| q), c) {
            (Some('\''), '\'') => *quote = None,
            (Some('\''), _) => {}
            (_, '\\') => {
                if chars.next().is_none() {
                    return (&raw[..at], true);
                }
            }
            (Some(_), '"') => *quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => *quote = Some((c, line)),
            (None, '#') if previous.is_none_or(|p| p == ' ' || p == '\t') => {
                return (&raw[..at], false);
            }
            (None, _) => {}
        }
        previous = Some(c);
    }
    (raw, false)
}

#[cfg(test)]
#[path = "shell_lines.test.rs"]
mod tests;
