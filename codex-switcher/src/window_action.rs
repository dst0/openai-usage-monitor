use clap::Subcommand;

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowAction {
    /// Show current saved window bounds and active window state
    Status,
    /// Save current Codex window position and size to disk
    Save,
    /// Restore saved window position and size to active Codex app
    Restore,
}
