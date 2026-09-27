use super::Commands;
use crate::cli::Cli;
use clap::Parser;

fn parse(args: &[&str]) -> Result<Option<Commands>, clap::Error> {
    Cli::try_parse_from(std::iter::once(&"codex-mon").chain(args)).map(|cli| cli.command)
}

#[test]
fn window_task_restoration_is_off_unless_requested() {
    match parse(&["restart"]).unwrap() {
        Some(Commands::Restart {
            restore_window_tasks,
            ..
        }) => assert!(!restore_window_tasks),
        _ => panic!("restart did not parse"),
    }
    match parse(&["restart", "--restore-window-tasks"]).unwrap() {
        Some(Commands::Restart {
            restore_window_tasks,
            ..
        }) => assert!(restore_window_tasks),
        _ => panic!("restart did not parse"),
    }
    match parse(&["switch", "work"]).unwrap() {
        Some(Commands::Switch {
            restore_window_tasks,
            ..
        }) => assert!(!restore_window_tasks),
        _ => panic!("switch did not parse"),
    }
    match parse(&["switch", "work", "--restore-window-tasks"]).unwrap() {
        Some(Commands::Switch {
            restore_window_tasks,
            ..
        }) => assert!(restore_window_tasks),
        _ => panic!("switch did not parse"),
    }
}

#[test]
fn window_task_restoration_needs_a_restart_and_takes_no_value() {
    assert!(parse(&["switch", "work", "--no-restart", "--restore-window-tasks"]).is_err());
    assert!(parse(&["restart", "--restore-window-tasks=false"]).is_err());
    assert!(parse(&["distribute", "--restore-window-tasks"]).is_err());
    assert!(parse(&["resume", "--restore-window-tasks"]).is_err());
}
