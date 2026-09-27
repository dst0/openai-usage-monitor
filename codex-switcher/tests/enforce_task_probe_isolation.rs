//! The window-task commands foreground ChatGPT windows, synthesize a
//! keystroke, let ChatGPT write task links to the clipboard, and (for the
//! rehearsal and restore) open windows and send task links. The probe and
//! the rehearsal must remain explicit CLI diagnostics. The snapshot and
//! restore commands may run only inside `cxi restart` and `cxi switch` when
//! the user passes `--restore-window-tasks`. Distribution, recovery, the
//! daemon, the Monitor app, launch agents, workflows, and scripts must never
//! reach any of them. This scan fails when any file outside the listed ones
//! names a command or its entry point, so that a new caller gets a
//! deliberate safety review instead of silently turning a diagnostic or an
//! explicit request into automation. It also fails when a listed file stops
//! naming it, so the list stays exact.

use std::fs;
use std::path::{Path, PathBuf};

const HELPER_MAIN: &str = "scripts/codex-window-restore.swift";
const COMMAND_ENUM: &str = "codex-switcher/src/distribution/window_task_command.rs";
const CLIENT_TEST: &str = "codex-switcher/src/distribution/window_task_helper_client.test.rs";
const PROBE_SERVICE: &str = "codex-switcher/src/distribution/window_task_probe_service.rs";
const PROBE_TEST: &str = "codex-switcher/src/distribution/window_task_probe_service.test.rs";
const RESTART_SESSION: &str = "codex-switcher/src/distribution/window_task_restart_session.rs";
const RESTART_TEST: &str = "codex-switcher/src/distribution/window_task_restart_session.test.rs";
const RESTART_STEPS: &str = "codex-switcher/src/switcher/restart_window_task_service.rs";
const RESTART_COMMAND: &str = "codex-switcher/src/switcher/recovery_command_service.rs";
const SWITCH_SERVICE: &str = "codex-switcher/src/switcher/account_switch_service.rs";
const SWITCH_COMMAND: &str = "codex-switcher/src/switch_command_service.rs";
const DESKTOP_COMMAND: &str = "codex-switcher/src/desktop_command_service.rs";
const DESKTOP_COMMAND_TEST: &str = "codex-switcher/src/desktop_command_service.test.rs";
const WINDOW_ACTION: &str = "codex-switcher/src/window_action.rs";
const WINDOW_ACTION_TEST: &str = "codex-switcher/src/window_action.test.rs";
const SHUTDOWN_GUARD: &str = "codex-switcher/src/switcher/desktop_shutdown_window_guard.rs";
const SHUTDOWN_GUARD_TEST: &str =
    "codex-switcher/src/switcher/desktop_shutdown_window_guard.test.rs";
const COMMANDS: &str = "codex-switcher/src/commands.rs";
const COMMANDS_TEST: &str = "codex-switcher/src/commands.test.rs";
const DISPATCHER: &str = "codex-switcher/src/command_dispatcher.rs";
const SWIFT_PROBE: &str = "scripts/CodexWindowTaskProbe.swift";
const SWIFT_SESSION: &str = "scripts/CodexWindowTaskSession.swift";

/// (token, the only repository files that may contain it). Tokens are
/// compared with whitespace removed because rustfmt may wrap a call.
const RULES: [(&str, &[&str]); 23] = [
    // Explicit diagnostics.
    (
        "\"probe-selected-tasks\"",
        &[HELPER_MAIN, COMMAND_ENUM, PROBE_TEST],
    ),
    (
        "\"rehearse-window-task-restore\"",
        &[HELPER_MAIN, COMMAND_ENUM, PROBE_TEST],
    ),
    ("WindowTaskCommand::Probe", &[PROBE_SERVICE, CLIENT_TEST]),
    ("WindowTaskCommand::Rehearse", &[PROBE_SERVICE, CLIENT_TEST]),
    ("probeSelectedTasks", &[SWIFT_PROBE, HELPER_MAIN]),
    ("rehearseWindowTaskRestore", &[SWIFT_SESSION, HELPER_MAIN]),
    (
        "WindowTaskProbe(system",
        &[SWIFT_PROBE, "tests/CodexWindowTaskProbeCoreTests.swift"],
    ),
    ("SystemWindowTaskProbe(", &[SWIFT_PROBE, SWIFT_SESSION]),
    (
        "WindowTaskProbeService::run(",
        &[DESKTOP_COMMAND, PROBE_TEST],
    ),
    (
        "WindowTaskProbeService::rehearse(",
        &[DESKTOP_COMMAND, PROBE_TEST],
    ),
    (
        "ProbeTasks",
        &[
            WINDOW_ACTION,
            WINDOW_ACTION_TEST,
            DESKTOP_COMMAND,
            DESKTOP_COMMAND_TEST,
        ],
    ),
    (
        "RehearseTaskRestore",
        &[
            WINDOW_ACTION,
            WINDOW_ACTION_TEST,
            DESKTOP_COMMAND,
            DESKTOP_COMMAND_TEST,
        ],
    ),
    ("probe-tasks", &[WINDOW_ACTION_TEST]),
    ("rehearse-task-restore", &[WINDOW_ACTION_TEST]),
    // Explicit restart requests.
    ("\"snapshot-window-tasks\"", &[HELPER_MAIN, COMMAND_ENUM]),
    ("\"restore-window-tasks\"", &[HELPER_MAIN, COMMAND_ENUM]),
    (
        "WindowTaskCommand::Snapshot",
        &[RESTART_SESSION, CLIENT_TEST],
    ),
    (
        "WindowTaskCommand::Restore",
        &[RESTART_SESSION, CLIENT_TEST],
    ),
    (
        "WindowTaskRestartSession::capture(",
        &[RESTART_STEPS, RESTART_TEST],
    ),
    (
        "RestartWindowTaskService::capture_if_requested(",
        &[
            RESTART_COMMAND,
            SWITCH_SERVICE,
            "codex-switcher/src/switcher/restart_window_task_service.test.rs",
        ],
    ),
    (
        "--restore-window-tasks",
        &[
            RESTART_SESSION,
            RESTART_STEPS,
            RESTART_COMMAND,
            SWITCH_COMMAND,
            SHUTDOWN_GUARD,
            SHUTDOWN_GUARD_TEST,
            COMMANDS_TEST,
            // Agent instructions: never add it without the user's request.
            "skills/cxi/SKILL.md",
            "skills/codex-mon/SKILL.md",
        ],
    ),
    (
        "restore_window_tasks",
        &[
            COMMANDS,
            COMMANDS_TEST,
            DISPATCHER,
            SWITCH_COMMAND,
            RESTART_COMMAND,
            SWITCH_SERVICE,
        ],
    ),
    (
        "allow-focus-and-clipboard",
        &[
            SWIFT_PROBE,
            COMMAND_ENUM,
            PROBE_SERVICE,
            "codex-switcher/src/distribution/window_task_helper_client.rs",
            CLIENT_TEST,
            PROBE_TEST,
            RESTART_TEST,
            WINDOW_ACTION_TEST,
            DESKTOP_COMMAND_TEST,
        ],
    ),
];
/// Directories that hold build output, history, other worktrees, or prose
/// rather than code. Skill instructions are installed for agents, so their
/// Markdown is scanned; other Markdown is documentation.
const SKIPPED_DIRECTORIES: [&str; 6] = [
    ".git",
    ".claude/worktrees",
    "target",
    "node_modules",
    ".build",
    "docs",
];
const SCANNED_MARKDOWN_ROOT: &str = "skills/";
const THIS_FILE: &str = "codex-switcher/tests/enforce_task_probe_isolation.rs";

#[test]
fn only_explicit_commands_reach_window_task_focus_and_clipboard() {
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
        "window-task callers changed; review any new one as a focus, window, and clipboard side effect:\n{}",
        violations.join("\n")
    );
}

/// Regular files only: symbolic links are not followed, so a link to a
/// live directory cannot pull untracked state into the scan.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                let skipped = SKIPPED_DIRECTORIES
                    .iter()
                    .any(|skipped| *skipped == name || *skipped == relative);
                if !skipped {
                    stack.push(path);
                }
            } else if kind.is_file()
                && (!name.ends_with(".md") || relative.starts_with(SCANNED_MARKDOWN_ROOT))
            {
                found.push(path);
            }
        }
    }
    found
}
