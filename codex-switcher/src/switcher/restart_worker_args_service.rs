use super::switch_trigger::SwitchTrigger;

/// Builds the argument list a detached restart worker reruns, so the
/// worker gets exactly the user's request: `--restore-window-tasks` only
/// when the user asked for it, and never with a non-user trigger.
pub(crate) struct RestartWorkerArgsService;

impl RestartWorkerArgsService {
    pub(crate) fn restart(
        delay_seconds: u64,
        primary_thread: Option<&str>,
        restore_window_tasks: bool,
    ) -> Vec<String> {
        let mut args = vec![
            "restart".into(),
            "--delay-seconds".into(),
            delay_seconds.max(5).to_string(),
        ];
        if let Some(id) = primary_thread {
            args.extend(["--primary-thread".into(), id.to_string()]);
        }
        if restore_window_tasks {
            args.push("--restore-window-tasks".into());
        }
        args
    }

    pub(crate) fn switch(
        account: &str,
        trigger: SwitchTrigger,
        restore_window_tasks: bool,
    ) -> Result<Vec<String>, String> {
        Self::check_window_task_request(trigger, restore_window_tasks)?;
        let mut args = vec![
            "switch".to_string(),
            account.to_string(),
            "--restart".into(),
        ];
        if trigger != SwitchTrigger::User {
            args.extend(["--trigger".into(), trigger.as_str().to_string()]);
        }
        if restore_window_tasks {
            args.push("--restore-window-tasks".into());
        }
        Ok(args)
    }

    /// Window-task restoration focuses the user's windows and uses their
    /// clipboard, so only the user may ask for it.
    pub(crate) fn check_window_task_request(
        trigger: SwitchTrigger,
        restore_window_tasks: bool,
    ) -> Result<(), String> {
        if restore_window_tasks && trigger != SwitchTrigger::User {
            return Err(
                "--restore-window-tasks is accepted only from a user-triggered switch".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "restart_worker_args_service.test.rs"]
mod tests;
