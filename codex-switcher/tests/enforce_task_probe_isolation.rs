//! The window task probe foregrounds ChatGPT windows, synthesizes a
//! keystroke, and replaces the clipboard. It must remain an explicit CLI
//! diagnostic: restart, distribution, recovery, the daemon, the Monitor app,
//! launch agents, workflows, and scripts must never reach it. This scan fails
//! when any file outside the listed ones names the probe, so that a new
//! caller gets a deliberate safety review instead of silently turning a
//! diagnostic into automation. It also fails when a listed file stops naming
//! it, so the list stays exact.

use std::fs;
use std::path::{Path, PathBuf};

/// (token, the only repository files that may contain it). Tokens are
/// compared with whitespace removed because rustfmt may wrap a call.
const RULES: [(&str, &[&str]); 9] = [
    (
        "probe-selected-tasks",
        &[
            "scripts/codex-window-restore.swift",
            "codex-switcher/src/distribution/system_window_restore_backend.rs",
            "codex-switcher/src/distribution/window_task_probe_service.test.rs",
        ],
    ),
    (
        "probe_selected_tasks",
        &[
            "codex-switcher/src/distribution/system_window_restore_backend.rs",
            "codex-switcher/src/distribution/window_task_probe_service.rs",
        ],
    ),
    (
        "probeSelectedTasks",
        &[
            "scripts/CodexWindowTaskProbe.swift",
            "scripts/codex-window-restore.swift",
        ],
    ),
    (
        "WindowTaskProbe(system",
        &[
            "scripts/CodexWindowTaskProbe.swift",
            "tests/CodexWindowTaskProbeCoreTests.swift",
        ],
    ),
    (
        "SystemWindowTaskProbe(",
        &["scripts/CodexWindowTaskProbe.swift"],
    ),
    (
        "WindowTaskProbeService::run(",
        &[
            "codex-switcher/src/desktop_command_service.rs",
            "codex-switcher/src/distribution/window_task_probe_service.test.rs",
        ],
    ),
    (
        "ProbeTasks",
        &[
            "codex-switcher/src/window_action.rs",
            "codex-switcher/src/window_action.test.rs",
            "codex-switcher/src/desktop_command_service.rs",
            "codex-switcher/src/desktop_command_service.test.rs",
        ],
    ),
    ("probe-tasks", &["codex-switcher/src/window_action.test.rs"]),
    (
        "allow-focus-and-clipboard",
        &[
            "scripts/CodexWindowTaskProbe.swift",
            "codex-switcher/src/distribution/system_window_restore_backend.rs",
            "codex-switcher/src/distribution/window_task_probe_service.rs",
            "codex-switcher/src/distribution/window_task_probe_service.test.rs",
            "codex-switcher/src/window_action.test.rs",
            "codex-switcher/src/desktop_command_service.test.rs",
        ],
    ),
];
/// Directories that hold build output, history, or prose rather than code.
const SKIPPED_DIRECTORIES: [&str; 6] = [
    ".git",
    ".claude",
    "target",
    "node_modules",
    ".build",
    "docs",
];
const THIS_FILE: &str = "codex-switcher/tests/enforce_task_probe_isolation.rs";

#[test]
fn only_the_explicit_cli_command_reaches_the_task_probe() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let files = files(&repo);
    assert!(files.len() > 400, "expected to scan the repository");
    let mut violations = Vec::new();
    let mut seen: Vec<Vec<String>> = vec![Vec::new(); RULES.len()];
    for path in &files {
        let relative = path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if relative == THIS_FILE {
            continue;
        }
        let compact: String = String::from_utf8_lossy(&fs::read(path).unwrap())
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        for (index, (token, allowed)) in RULES.iter().enumerate() {
            if compact.contains(token) {
                seen[index].push(relative.clone());
                if !allowed.contains(&relative.as_str()) {
                    violations.push(format!("{relative} names {token}"));
                }
            }
        }
    }
    for (index, (token, allowed)) in RULES.iter().enumerate() {
        for file in allowed
            .iter()
            .filter(|file| !seen[index].contains(&file.to_string()))
        {
            violations.push(format!("{file} no longer names {token}; update this scan"));
        }
    }
    assert!(
        violations.is_empty(),
        "the window task probe's callers changed; review any new one as a focus and clipboard side effect:\n{}",
        violations.join("\n")
    );
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if !SKIPPED_DIRECTORIES.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if !name.ends_with(".md") {
                found.push(path);
            }
        }
    }
    found
}
