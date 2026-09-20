use super::commands::Commands;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "codex-mon",
    version = "0.1.0",
    about = "OpenAI Codex Account Switcher & Quota Monitor"
)]
pub(super) struct Cli {
    /// Internal one-shot invocation; unlike environment variables, this is not inherited.
    #[arg(long, hide = true)]
    pub(super) restart_worker: bool,
    #[command(subcommand)]
    pub(super) command: Option<Commands>,
}
