use super::DesktopCommandService;
use crate::window_action::WindowAction;

/// Without the flag the command must refuse before it resolves the Codex
/// home, takes the operation lock, reads the process table, or resolves the
/// helper. A coordinator that forwarded `true` would reach
/// `storage::codex_home()`, which panics in test builds without a guard.
#[test]
fn task_probe_command_refuses_without_the_opt_in_flag() {
    assert_eq!(
        DesktopCommandService::window(Some(WindowAction::ProbeTasks {
            allow_focus_and_clipboard: false,
        })),
        Err("Explicit --allow-focus-and-clipboard is required".into())
    );
}
