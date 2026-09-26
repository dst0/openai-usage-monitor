//! Cargo must build exactly the committed `Cargo.lock`. `--locked` (or
//! `--frozen`, which implies it) makes cargo fail when the lockfile is missing
//! or would change, instead of silently resolving whatever crates.io serves
//! that day. The rule covers workflows and the repository's shell scripts,
//! because the installer builds the binary users run.
//!
//! Shell text is read a line at a time: `\` continuations are joined, `#`
//! comments and YAML `name:` labels are skipped, and each simple command
//! between `;`, `&`, `|`, parentheses, and backticks is checked on its own.
//! A flag after a bare `--` belongs to the program cargo runs, not to cargo.
//! Shapes that never resolve dependencies are exempt: `cargo --version`,
//! `cargo -V`, `cargo clippy --version`, any `cargo fmt` (which rejects
//! `--locked`), and lookups such as `command -v cargo`. Indirect invocations
//! (`$CARGO`, `eval`, cargo aliases) are outside this scan; on CI,
//! `ci_tests_ran_against_committed_lockfile` checks the lockfile itself.

use crate::yaml_lines::{entry, is_content};

/// Flags that stop cargo from creating or rewriting `Cargo.lock`.
const LOCK_FLAGS: [&str; 2] = ["--locked", "--frozen"];
/// Complete argument lists that only print a version.
const VERSION_QUERIES: [&[&str]; 4] = [
    &["--version"],
    &["-V"],
    &["clippy", "--version"],
    &["clippy", "-V"],
];
/// Commands that print where `cargo` is instead of running it.
const LOOKUP_COMMANDS: [&str; 4] = ["which", "type", "hash", "whereis"];
/// Characters that end a simple shell command.
const COMMAND_SEPARATORS: [char; 6] = [';', '&', '|', '(', ')', '`'];

pub fn unlocked_cargo_violations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (number, line) in logical_lines(text) {
        for words in simple_commands(&line) {
            for at in cargo_invocations(&words) {
                if !is_locked_or_exempt(&words[at + 1..]) {
                    out.push(format!(
                        "line {number}: `{}` must pass `--locked` so a missing or stale \
                         Cargo.lock fails instead of being re-resolved",
                        words[at..].join(" ")
                    ));
                }
            }
        }
    }
    out
}

/// `(1-based first line, text)` of each line that can run a command, with `\`
/// continuations joined and trailing comments removed. A YAML `name:` is a
/// display label and never runs.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (i, raw) in text.lines().enumerate() {
        let label = entry(raw).is_some_and(|e| e.key == "name");
        if pending.is_none() && (!is_content(raw) || label) {
            continue;
        }
        let code = raw.split(" #").next().unwrap_or(raw).trim_end();
        let (start, mut joined) = pending.take().unwrap_or((i + 1, String::new()));
        joined.push(' ');
        match code.strip_suffix('\\') {
            Some(head) => {
                joined.push_str(head);
                pending = Some((start, joined));
            }
            None => {
                joined.push_str(code);
                out.push((start, joined));
            }
        }
    }
    out.extend(pending);
    out
}

/// Words of each simple command on `line`, unquoted, with redirections and
/// their targets dropped.
fn simple_commands(line: &str) -> Vec<Vec<String>> {
    line.split(COMMAND_SEPARATORS)
        .map(words)
        .filter(|words| !words.is_empty())
        .collect()
}

fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_target = false;
    for word in command.split_whitespace() {
        let word = word.trim_matches(['"', '\'']);
        if std::mem::take(&mut skip_target) {
            continue;
        }
        if word.contains(['<', '>']) {
            // `>` alone or `2>` takes the next word as its target.
            skip_target = word.ends_with(['<', '>']);
            continue;
        }
        out.push(word.to_string());
    }
    out
}

/// Positions of words that run cargo: `cargo` itself or a path ending in
/// `/cargo`, unless the command only looks it up.
fn cargo_invocations(words: &[String]) -> Vec<usize> {
    (0..words.len())
        .filter(|&i| words[i].rsplit('/').next() == Some("cargo") && !is_lookup(&words[..i]))
        .collect()
}

/// Whether the words before `cargo` only look it up: `which cargo`,
/// `type -P cargo`, or `command -v cargo`. `command cargo` runs it.
fn is_lookup(before: &[String]) -> bool {
    let flags: Vec<&str> = before
        .iter()
        .rev()
        .map(String::as_str)
        .take_while(|w| w.starts_with('-'))
        .collect();
    let command = before
        .len()
        .checked_sub(flags.len() + 1)
        .map(|i| before[i].as_str());
    match command {
        Some("command") => flags.iter().any(|f| matches!(*f, "-v" | "-V")),
        Some(command) => LOOKUP_COMMANDS.contains(&command),
        None => false,
    }
}

fn is_locked_or_exempt(args: &[String]) -> bool {
    let own: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .take_while(|a| *a != "--")
        .collect();
    own.first() == Some(&"fmt")
        || VERSION_QUERIES.contains(&own.as_slice())
        || own.iter().any(|a| LOCK_FLAGS.contains(a))
}

#[cfg(test)]
#[path = "locked_cargo.test.rs"]
mod tests;
