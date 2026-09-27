use super::RestartWorkerArgsService as Args;
use crate::switcher::SwitchTrigger;

const FLAG: &str = "--restore-window-tasks";

#[test]
fn the_restart_worker_gets_the_flag_only_when_requested() {
    assert_eq!(
        Args::restart(0, None, false),
        ["restart", "--delay-seconds", "5"]
    );
    assert_eq!(
        Args::restart(9, Some("01a00000-0000-4000-8000-00000000000a"), true),
        [
            "restart",
            "--delay-seconds",
            "9",
            "--primary-thread",
            "01a00000-0000-4000-8000-00000000000a",
            FLAG
        ]
    );
    assert!(!Args::restart(9, Some("x"), false)
        .iter()
        .any(|arg| arg == FLAG));
}

#[test]
fn the_switch_worker_gets_the_flag_only_when_the_user_requested_it() {
    assert_eq!(
        Args::switch("work", SwitchTrigger::User, false).unwrap(),
        ["switch", "work", "--restart"]
    );
    assert_eq!(
        Args::switch("work", SwitchTrigger::User, true).unwrap(),
        ["switch", "work", "--restart", FLAG]
    );
    assert_eq!(
        Args::switch("work", SwitchTrigger::Auto, false).unwrap(),
        ["switch", "work", "--restart", "--trigger", "auto"]
    );
    for trigger in [SwitchTrigger::Auto, SwitchTrigger::Shim] {
        assert!(Args::switch("work", trigger, true).is_err());
        assert!(Args::check_window_task_request(trigger, true).is_err());
        assert!(Args::check_window_task_request(trigger, false).is_ok());
    }
    assert!(Args::check_window_task_request(SwitchTrigger::User, true).is_ok());
}
