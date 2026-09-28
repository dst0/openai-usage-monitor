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

/// The Menu Bar app's Auto-Switch Settings toggles run these exact `config` flags
/// (`Sources/CodexClient.swift`); a renamed flag would make a toggle fail silently.
#[test]
fn menu_bar_setting_flags_parse_as_booleans() {
    for value in [true, false] {
        let text = if value { "true" } else { "false" };
        let args = [
            "config",
            "--restart-app-on-switch",
            text,
            "--auto-switch-enabled",
            text,
            "--auto-switch-business-only",
            text,
            "--auto-switch-business-priority",
            text,
            "--preserve-window-bounds",
            text,
        ];
        match parse(&args).unwrap() {
            Some(Commands::Config {
                restart_app_on_switch,
                auto_switch_enabled,
                auto_switch_business_only,
                auto_switch_business_priority,
                preserve_window_bounds,
                ..
            }) => {
                assert_eq!(restart_app_on_switch, Some(value));
                assert_eq!(auto_switch_enabled, Some(value));
                assert_eq!(auto_switch_business_only, Some(value));
                assert_eq!(auto_switch_business_priority, Some(value));
                assert_eq!(preserve_window_bounds, Some(value));
            }
            _ => panic!("config did not parse"),
        }
    }
    assert!(parse(&["config", "--preserve-window-bounds"]).is_err());
    assert!(parse(&["config", "--preserve-window-bounds", "maybe"]).is_err());
}

/// The weekly reset submenu saves both values in one `config` run.
#[test]
fn menu_bar_weekly_reset_flags_parse_within_range() {
    let args = [
        "config",
        "--auto-reset-weekly-enabled",
        "true",
        "--auto-reset-weekly-min-hours",
        "167",
    ];
    match parse(&args).unwrap() {
        Some(Commands::Config {
            auto_reset_weekly_enabled,
            auto_reset_weekly_min_hours,
            ..
        }) => {
            assert_eq!(auto_reset_weekly_enabled, Some(true));
            assert_eq!(auto_reset_weekly_min_hours, Some(167));
        }
        _ => panic!("config did not parse"),
    }
    assert!(parse(&["config", "--auto-reset-weekly-min-hours", "168"]).is_err());
}
