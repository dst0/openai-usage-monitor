#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionStatus {
    NoActionNeeded,
    Success,
    PartialSuccess,
    DeferredCooldown,
    DeferredInFlight,
    Failed,
}

impl DistributionStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    pub fn is_partial_success(&self) -> bool {
        matches!(self, Self::PartialSuccess)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Success | Self::PartialSuccess | Self::NoActionNeeded
        )
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DistributionOutcome {
    pub operation_id: String,
    pub status: DistributionStatus,
    pub trigger: String,
    pub reason: String,
    pub current_app_id: Option<String>,
    pub current_cli_id: Option<String>,
    pub target_app_id: Option<String>,
    pub target_cli_id: Option<String>,
    pub restarted_desktop: bool,
    pub recovery_error: Option<String>,
    pub message: String,
}

impl DistributionOutcome {
    pub fn no_action(
        operation_id: impl Into<String>,
        trigger: impl Into<String>,
        reason: impl Into<String>,
        app: Option<String>,
        cli: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            status: DistributionStatus::NoActionNeeded,
            trigger: trigger.into(),
            reason: reason.into(),
            current_app_id: app.clone(),
            current_cli_id: cli.clone(),
            target_app_id: app,
            target_cli_id: cli,
            restarted_desktop: false,
            recovery_error: None,
            message: message.into(),
        }
    }

    pub fn deferred_cooldown(
        operation_id: impl Into<String>,
        trigger: impl Into<String>,
        reason: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            status: DistributionStatus::DeferredCooldown,
            trigger: trigger.into(),
            reason: reason.into(),
            current_app_id: None,
            current_cli_id: None,
            target_app_id: None,
            target_cli_id: None,
            restarted_desktop: false,
            recovery_error: None,
            message: message.into(),
        }
    }

    pub fn deferred_in_flight(
        operation_id: impl Into<String>,
        trigger: impl Into<String>,
        reason: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            status: DistributionStatus::DeferredInFlight,
            trigger: trigger.into(),
            reason: reason.into(),
            current_app_id: None,
            current_cli_id: None,
            target_app_id: None,
            target_cli_id: None,
            restarted_desktop: false,
            recovery_error: None,
            message: message.into(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn should_caller_retry(&self) -> bool {
        false
    }
}
