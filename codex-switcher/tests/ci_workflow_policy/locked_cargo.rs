//! Cargo must build exactly the committed `Cargo.lock`. `--locked` (or
//! `--frozen`, which implies it) makes cargo fail when the lockfile is missing
//! or would change, instead of silently resolving whatever crates.io serves
//! that day. The rule covers workflows and the repository's shell scripts,
//! because the installer builds the binary users run.
//!
//! `shell_lines.rs` turns the text into command lines. Each simple command
//! between `;`, `&`, `|`, parentheses, and backticks is checked on its own,
//! with quotes and backslashes removed from its words as the shell removes
//! them. Separators split even inside quotes, so `eval "cargo build"` and
//! `"$(cargo metadata)"` are checked too. A command runs cargo when a word is
//! `cargo` or a path ending in `/cargo`, when a `$(…)` or backtick lookup such
//! as `$(command -v cargo)` stands in its place, or when a variable whose name
//! contains `CARGO` (`$CARGO`, `"${MY_CARGO}"`) is the command word. A flag
//! after a bare `--` belongs to the program cargo runs, not to cargo. Shapes
//! that never resolve dependencies are exempt: `cargo --version`, `cargo -V`,
//! `cargo clippy --version`, any `cargo fmt` (which rejects `--locked`), and
//! lookups such as `command -v cargo`.
//!
//! Known limits, for the owner's own files rather than hostile input:
//! - Missed: a command built at run time (`eval "$cmd"`, a cargo alias, a
//!   `${{ matrix.cmd }}` expansion of a YAML flow item such as
//!   `['cargo test']`), `bash -c "cargo build" --locked` (the flag goes to
//!   bash), a `\` continuation that joins the next YAML key, an option value
//!   that is a lookup word (`sudo -u type cargo build`), and third-party
//!   actions that run cargo themselves. On CI,
//!   `ci_tests_ran_against_committed_lockfile` backs this scan up by checking
//!   the lockfile itself.
//! - Reported although harmless (fail closed): prose containing a bare
//!   lowercase `cargo` word in `echo`, heredocs, `if:` expressions, or YAML
//!   values; paths ending in `/cargo` used as data (`[ -x ~/.cargo/bin/cargo ]`);
//!   `cargo -vV` and `cargo --list`; a redirect attached to a flag
//!   (`--version>/dev/null` drops the whole word); and `--locked` after
//!   `2>&1` or `&>`, where `&` splits the command. Reword or restructure such
//!   lines.

use crate::shell_lines::ShellLines;

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
/// Words that may precede the command word: wrappers that run the command
/// after their own arguments, and shell keywords.
const COMMAND_PREFIXES: [&str; 17] = [
    "env", "time", "command", "exec", "sudo", "nice", "nohup", "xargs", "if", "then", "else",
    "elif", "do", "while", "until", "!", "{",
];
/// Characters that end a simple shell command.
const COMMAND_SEPARATORS: [char; 6] = [';', '&', '|', '(', ')', '`'];

pub fn unlocked_cargo_violations(text: &str) -> Vec<String> {
    let read = ShellLines::read(text);
    let mut out: Vec<String> = cargo_commands_in(&read)
        .into_iter()
        .filter(|(_, words)| !is_locked_or_exempt(&words[1..]))
        .map(|(number, words)| {
            format!(
                "line {number}: `{}` must pass `--locked` so a missing or stale \
                 Cargo.lock fails instead of being re-resolved",
                words.join(" ")
            )
        })
        .collect();
    if let Some(number) = read.open_quote_line {
        out.push(format!(
            "line {number}: quote is never closed, so the cargo scan cannot tell \
             commands from quoted text"
        ));
    }
    out
}

/// `(1-based line, words)` of every cargo invocation in `text`, starting at
/// the word that runs cargo.
pub fn cargo_commands(text: &str) -> Vec<(usize, Vec<String>)> {
    cargo_commands_in(&ShellLines::read(text))
}

/// Whether `text` runs `cargo <subcommand>` with `--locked` or `--frozen`
/// among cargo's own arguments, for rules that require a build to exist.
pub fn runs_locked(text: &str, subcommand: &str) -> bool {
    cargo_commands(text).iter().any(|(_, words)| {
        words.get(1).map(String::as_str) == Some(subcommand)
            && own_arguments(&words[1..])
                .iter()
                .any(|a| LOCK_FLAGS.contains(a))
    })
}

fn cargo_commands_in(read: &ShellLines) -> Vec<(usize, Vec<String>)> {
    let mut out = Vec::new();
    for (number, line) in &read.lines {
        for words in simple_commands(&substitute_cargo_lookups(line)) {
            for at in cargo_invocations(&words) {
                out.push((*number, words[at..].to_vec()));
            }
        }
    }
    out
}

/// `line` with each `$(…)` or backtick substitution that only looks cargo up
/// (`$(command -v cargo)`) replaced by `cargo`, the program it yields.
fn substitute_cargo_lookups(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find(['$', '`']) {
        let (opener, closer) = if rest[open..].starts_with("$(") {
            ("$(", ')')
        } else if rest[open..].starts_with('`') {
            ("`", '`')
        } else {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        };
        let body = &rest[open + opener.len()..];
        match body.find(closer) {
            Some(close) if is_cargo_lookup(&words(&body[..close])) => {
                out.push_str(&rest[..open]);
                out.push_str("cargo");
                rest = &body[close + 1..];
            }
            _ => {
                out.push_str(&rest[..open + opener.len()]);
                rest = body;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Words of each simple command on `line`, with quotes and backslashes
/// removed and redirections and their targets dropped.
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
        let word: String = word.chars().filter(|c| !"\"'\\".contains(*c)).collect();
        if std::mem::take(&mut skip_target) {
            continue;
        }
        if word.contains(['<', '>']) {
            // `>` alone or `2>` takes the next word as its target.
            skip_target = word.ends_with(['<', '>']);
            continue;
        }
        if !word.is_empty() {
            out.push(word);
        }
    }
    out
}

/// Positions of words that run cargo, unless the command only looks it up.
fn cargo_invocations(words: &[String]) -> Vec<usize> {
    (0..words.len())
        .filter(|&i| {
            let before = &words[..i];
            let runs = words[i].rsplit('/').next() == Some("cargo")
                || (is_cargo_variable(&words[i]) && is_command_position(before));
            runs && !is_lookup(before)
        })
        .collect()
}

/// `$NAME` or `${NAME…}` without a path, where `NAME` contains `CARGO`.
fn is_cargo_variable(word: &str) -> bool {
    let Some(name) = word
        .strip_prefix("${")
        .or_else(|| word.strip_prefix('$'))
        .filter(|_| !word.contains('/'))
    else {
        return false;
    };
    let name: String = name
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    name.to_ascii_uppercase().contains("CARGO")
}

/// Whether the next word is the command word: everything before it is a
/// `NAME=value` assignment, an option, a wrapper such as `env` or `sudo`, or a
/// shell keyword such as `if` or `!`.
fn is_command_position(before: &[String]) -> bool {
    before
        .iter()
        .all(|w| w.starts_with('-') || COMMAND_PREFIXES.contains(&w.as_str()) || is_assignment(w))
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Whether `words` is a lookup of cargo such as `command -v cargo`.
fn is_cargo_lookup(words: &[String]) -> bool {
    words
        .split_last()
        .is_some_and(|(last, before)| last.rsplit('/').next() == Some("cargo") && is_lookup(before))
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

/// Arguments cargo itself reads: those before a bare `--`.
fn own_arguments(args: &[String]) -> Vec<&str> {
    args.iter()
        .map(String::as_str)
        .take_while(|a| *a != "--")
        .collect()
}

fn is_locked_or_exempt(args: &[String]) -> bool {
    let own = own_arguments(args);
    own.first() == Some(&"fmt")
        || VERSION_QUERIES.contains(&own.as_slice())
        || own.iter().any(|a| LOCK_FLAGS.contains(a))
}

#[cfg(test)]
#[path = "locked_cargo.test.rs"]
mod tests;
