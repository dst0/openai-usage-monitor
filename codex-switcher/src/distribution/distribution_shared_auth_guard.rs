use super::app_lifecycle::AppLifecycle;
use super::distribution_plan::DistributionPlan;

/// ChatGPT and cxi use the same auth.json. Split targets are impossible even
/// while Desktop is closed; a live Desktop also forbids CLI-only rewrites.
pub(super) struct DistributionSharedAuthGuard;

impl DistributionSharedAuthGuard {
    pub(super) fn before_journal(
        lifecycle: &dyn AppLifecycle,
        plan: &DistributionPlan,
    ) -> Result<(), String> {
        let running = lifecycle.is_app_running()?;
        match (plan.target_app_id.as_deref(), plan.target_cli_id.as_deref()) {
            (Some(app), Some(cli)) if !app.eq_ignore_ascii_case(cli) => {
                return Err("Desktop and CLI cannot use different target accounts".into());
            }
            (Some(_), Some(_)) => {}
            _ if plan.has_changes() || running => {
                return Err("Shared authentication target account is unavailable".into());
            }
            _ => {}
        }
        if plan.restart_required && !running {
            return Err("Desktop state changed before account distribution".into());
        }
        if !running {
            return Ok(());
        }
        if !plan.restart_required && plan.has_changes() {
            return Err("A running Desktop must stop before shared authentication changes".into());
        }
        Ok(())
    }

    pub(super) fn require_desktop_stopped(lifecycle: &dyn AppLifecycle) -> Result<(), String> {
        if lifecycle.is_app_running()? {
            return Err("Desktop is running while shared authentication would change".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "distribution_shared_auth_guard.test.rs"]
mod tests;
