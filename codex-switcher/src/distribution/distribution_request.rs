use super::distribution_trigger::DistributionTrigger;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DistributionRequest {
    pub trigger: DistributionTrigger,
    pub reason: String,
    pub preferred_app_id: Option<String>,
    pub preferred_cli_id: Option<String>,
    pub force_restart: bool,
    pub allow_restart: bool,
    pub dry_run: bool,
}

impl DistributionRequest {
    pub fn auto(reason: impl Into<String>) -> Self {
        Self {
            trigger: DistributionTrigger::Auto,
            reason: reason.into(),
            preferred_app_id: None,
            preferred_cli_id: None,
            force_restart: false,
            allow_restart: true,
            dry_run: false,
        }
    }

    pub fn user(reason: impl Into<String>) -> Self {
        Self {
            trigger: DistributionTrigger::User,
            reason: reason.into(),
            preferred_app_id: None,
            preferred_cli_id: None,
            force_restart: false,
            allow_restart: true,
            dry_run: false,
        }
    }

    pub fn with_preferred_app(mut self, app_id: Option<String>) -> Self {
        self.preferred_app_id = app_id;
        self
    }

    pub fn with_preferred_cli(mut self, cli_id: Option<String>) -> Self {
        self.preferred_cli_id = cli_id;
        self
    }

    pub fn with_allow_restart(mut self, allow: bool) -> Self {
        self.allow_restart = allow;
        self
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}
