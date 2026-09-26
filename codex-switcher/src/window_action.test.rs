use super::WindowAction;
use crate::cli::Cli;
use crate::commands::Commands;
use clap::Parser;

fn window_action(args: &[&str]) -> Result<Option<WindowAction>, clap::Error> {
    let cli = Cli::try_parse_from(["codex-mon", "window"].iter().chain(args))?;
    match cli.command {
        Some(Commands::Window { action }) => Ok(action),
        _ => panic!("`window` did not parse as the window command"),
    }
}

#[test]
fn task_probe_opt_in_defaults_to_refused() {
    assert_eq!(
        window_action(&["probe-tasks"]).unwrap(),
        Some(WindowAction::ProbeTasks {
            allow_focus_and_clipboard: false
        })
    );
    assert_eq!(
        window_action(&["probe-tasks", "--allow-focus-and-clipboard"]).unwrap(),
        Some(WindowAction::ProbeTasks {
            allow_focus_and_clipboard: true
        })
    );
}

#[test]
fn task_probe_opt_in_takes_no_value_and_belongs_only_to_the_probe() {
    assert!(window_action(&["probe-tasks", "--allow-focus-and-clipboard=false"]).is_err());
    assert!(window_action(&["probe-tasks", "--allow-focus-and-clipboard", "yes"]).is_err());
    assert!(window_action(&["restore", "--allow-focus-and-clipboard"]).is_err());
    assert_eq!(window_action(&[]).unwrap(), None);
}
