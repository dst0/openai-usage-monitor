use clap::Subcommand;

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowAction {
    /// Show current saved window bounds and active window state
    Status,
    /// Save current Codex window position and size to disk
    Save,
    /// Restore saved window position and size to active Codex app
    Restore,
    /// Explicit diagnostic: focus each Desktop window and copy its selected chat link
    ProbeTasks {
        /// Allow the probe to foreground ChatGPT and replace the clipboard
        #[arg(long)]
        allow_focus_and_clipboard: bool,
    },
}

#[cfg(test)]
#[path = "window_action.test.rs"]
mod tests;
