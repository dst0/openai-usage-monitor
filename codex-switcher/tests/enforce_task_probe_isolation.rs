//! The window task probe foregrounds ChatGPT windows, synthesizes a
//! keystroke, and replaces the clipboard. It must remain an explicit CLI
//! diagnostic: restart, distribution, recovery, the daemon, the Monitor app,
//! and the install scripts must never reach it. This scan fails when a new
//! caller appears, so that caller gets a deliberate safety review instead of
//! silently turning a diagnostic into automation.

use std::fs;
use std::path::{Path, PathBuf};

/// (pattern, the only files that may contain it). Patterns are compared with
/// whitespace removed because rustfmt may wrap a call.
const RUST_RULES: [(&str, &[&str]); 4] = [
    (
        "probe_selected_tasks(",
        &[
            "distribution/system_window_restore_backend.rs",
            "distribution/window_task_probe_service.rs",
        ],
    ),
    (
        "\"probe-selected-tasks\"",
        &["distribution/system_window_restore_backend.rs"],
    ),
    (
        "WindowTaskProbeService::run(",
        &[
            "desktop_command_service.rs",
            "distribution/window_task_probe_service.test.rs",
        ],
    ),
    (
        "WindowAction::ProbeTasks",
        &["desktop_command_service.rs", "window_action.test.rs"],
    ),
];

const NATIVE_RULES: [(&str, &[&str]); 3] = [
    (
        "probeSelectedTasks(",
        &[
            "scripts/CodexWindowTaskProbe.swift",
            "scripts/codex-window-restore.swift",
        ],
    ),
    (
        "\"probe-selected-tasks\"",
        &["scripts/codex-window-restore.swift"],
    ),
    ("probe-tasks", &[]),
];

#[test]
fn only_the_explicit_cli_command_reaches_the_task_probe() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = crate_root.join("src");
    let repo = crate_root.join("..");
    let rust = files(&src, &["rs"]);
    let native: Vec<PathBuf> = files(&repo.join("Sources"), &["swift", "sh"])
        .into_iter()
        .chain(files(&repo.join("scripts"), &["swift", "sh", "js"]))
        .collect();
    assert!(rust.len() > 100, "expected to scan the crate");
    assert!(native.len() > 20, "expected to scan the native sources");

    let mut violations = scan(&rust, &src, &RUST_RULES);
    violations.extend(scan(&native, &repo, &NATIVE_RULES));
    assert!(
        violations.is_empty(),
        "the window task probe gained a caller; review it as a focus/clipboard side effect:\n{}",
        violations.join("\n")
    );
}

fn scan(files: &[PathBuf], root: &Path, rules: &[(&str, &[&str])]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut seen = vec![false; rules.len()];
    for path in files {
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let compact: String = fs::read_to_string(path)
            .unwrap()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        for (index, (pattern, allowed)) in rules.iter().enumerate() {
            if compact.contains(pattern) {
                seen[index] = true;
                if !allowed.contains(&relative.as_str()) {
                    violations.push(format!("{relative} contains {pattern}"));
                }
            }
        }
    }
    for (index, (pattern, allowed)) in rules.iter().enumerate() {
        // A rule whose allowed files no longer mention it has gone stale.
        if !allowed.is_empty() && !seen[index] {
            violations.push(format!("no file contains {pattern}; update this scan"));
        }
    }
    violations
}

fn files(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| extensions.iter().any(|allowed| ext == *allowed))
            {
                found.push(path);
            }
        }
    }
    found
}
