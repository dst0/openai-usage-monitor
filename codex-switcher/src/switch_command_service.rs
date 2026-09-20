use crate::{distribution, storage, switcher};

pub(super) struct SwitchCommandService;

impl SwitchCommandService {
    pub(super) fn switch_account(
        account: String,
        no_restart: bool,
        restart: bool,
        trigger: String,
    ) -> Result<(), String> {
        let accounts = storage::load_accounts().unwrap_or_default();
        let should_restart = (restart || accounts.settings.restart_app_on_switch) && !no_restart;
        let notify = accounts.settings.notify_on_switch;
        let switch_trigger: switcher::SwitchTrigger =
            trigger.parse().unwrap_or(switcher::SwitchTrigger::User);
        println!("🔄 Switching to account '{}'...", account);
        let dispatch = if should_restart && switcher::is_codex_app_running() {
            let mut restart_args = vec![
                "switch".to_string(),
                account.clone(),
                "--restart".to_string(),
            ];
            if switch_trigger != switcher::SwitchTrigger::User {
                restart_args.push("--trigger".to_string());
                restart_args.push(switch_trigger.as_str().to_string());
            }
            switcher::dispatch_self_restart(&restart_args)
        } else {
            Ok(false)
        };
        match dispatch.and_then(|scheduled| {
            if scheduled {
                return Ok((true, None));
            }
            switcher::switch_to_account(&account, should_restart, notify, switch_trigger)
                .map(|outcome| (false, outcome.recovery_error))
        }) {
            Ok((scheduled, recovery_error)) => {
                if let Some(error) = recovery_error {
                    eprintln!(
                        "⚠️ Account '{}' was switched, but desktop recovery is incomplete: {}",
                        account, error
                    );
                } else if !scheduled {
                    println!("✅ Successfully switched to account '{}'!", account);
                }
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn distribute(
        trigger: String,
        reason: String,
        dry_run: bool,
        app_target: Option<String>,
        cli_target: Option<String>,
        no_restart: bool,
        json: bool,
    ) -> Result<(), String> {
        let parsed_trigger: distribution::DistributionTrigger = trigger
            .parse()
            .unwrap_or(distribution::DistributionTrigger::User);
        let mut request = match parsed_trigger {
            distribution::DistributionTrigger::Auto => {
                distribution::DistributionRequest::auto(reason)
            }
            distribution::DistributionTrigger::User => {
                distribution::DistributionRequest::user(reason)
            }
        };
        request = request
            .with_preferred_app(app_target)
            .with_preferred_cli(cli_target)
            .with_allow_restart(!no_restart)
            .with_dry_run(dry_run);

        let outcome = distribution::DistributionCoordinator::new().execute(request)?;
        if json {
            if let Ok(serialized) = serde_json::to_string_pretty(&outcome) {
                println!("{}", serialized);
            }
            return Ok(());
        }
        match outcome.status {
            distribution::DistributionStatus::NoActionNeeded
            | distribution::DistributionStatus::DeferredCooldown
            | distribution::DistributionStatus::DeferredInFlight => {
                println!("ℹ️ {}", outcome.message);
            }
            distribution::DistributionStatus::Success => {
                println!("✅ {}", outcome.message);
                if let Some(app) = &outcome.target_app_id {
                    println!("   🖥️ Desktop App: {}", app);
                }
                if let Some(cli) = &outcome.target_cli_id {
                    println!("   > CLI: {}", cli);
                }
            }
            distribution::DistributionStatus::PartialSuccess => {
                eprintln!("⚠️ {}", outcome.message);
                if let Some(error) = &outcome.recovery_error {
                    eprintln!("   ↳ Recovery error: {}", error);
                }
            }
            distribution::DistributionStatus::Failed => {
                eprintln!("❌ {}", outcome.message);
                std::process::exit(1);
            }
        }
        Ok(())
    }
}
