/// The native helper's window-task commands. Each one focuses ChatGPT
/// windows and lets ChatGPT write task links to the clipboard, so the helper
/// refuses each unless it receives `--allow-focus-and-clipboard yes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowTaskCommand {
    /// Explicit diagnostic: counts distinct task links, never returns them.
    Probe,
    /// Before an explicitly requested restart: every window's task and frame.
    Snapshot,
    /// After that restart's relaunch, and again after recovery.
    Restore,
    /// Explicit diagnostic: rehearses the restore steps in extra windows.
    Rehearse,
}

impl WindowTaskCommand {
    pub(super) fn helper_command(self) -> &'static str {
        match self {
            Self::Probe => "probe-selected-tasks",
            Self::Snapshot => "snapshot-window-tasks",
            Self::Restore => "restore-window-tasks",
            Self::Rehearse => "rehearse-window-task-restore",
        }
    }

    const ALL: [Self; 4] = [Self::Probe, Self::Snapshot, Self::Restore, Self::Rehearse];

    /// Whether `error` is a named helper failure, which already says whether
    /// it followed a focus change.
    pub(super) fn is_named_failure(error: &str) -> bool {
        Self::ALL.iter().any(|command| {
            error
                .strip_prefix(command.label())
                .is_some_and(|rest| rest.starts_with(" failed: "))
        })
    }

    /// Names the operation in every failure the helper reports.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Probe => "Task probe",
            Self::Snapshot => "Window task snapshot",
            Self::Restore => "Window task restore",
            Self::Rehearse => "Window task rehearsal",
        }
    }
}
