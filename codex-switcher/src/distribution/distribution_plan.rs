use super::distribution_candidate::DistributionCandidate;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DistributionPlan {
    pub current_app_id: Option<String>,
    pub current_cli_id: Option<String>,
    pub target_app_id: Option<String>,
    pub target_cli_id: Option<String>,
    pub app_switch_needed: bool,
    pub cli_switch_needed: bool,
    pub restart_required: bool,
    pub decision_reason: String,
    pub evaluated_candidates: Vec<DistributionCandidate>,
}

impl DistributionPlan {
    pub fn no_action(
        current_app: Option<String>,
        current_cli: Option<String>,
        reason: impl Into<String>,
        candidates: Vec<DistributionCandidate>,
    ) -> Self {
        Self {
            target_app_id: current_app.clone(),
            target_cli_id: current_cli.clone(),
            current_app_id: current_app,
            current_cli_id: current_cli,
            app_switch_needed: false,
            cli_switch_needed: false,
            restart_required: false,
            decision_reason: reason.into(),
            evaluated_candidates: candidates,
        }
    }

    pub fn has_changes(&self) -> bool {
        self.app_switch_needed || self.cli_switch_needed
    }

    pub fn candidate_summary(&self) -> String {
        self.evaluated_candidates
            .iter()
            .map(|candidate| {
                if candidate.eligible {
                    format!("{}:eligible", candidate.sanitized_label)
                } else {
                    format!(
                        "{}:skip({})",
                        candidate.sanitized_label,
                        candidate.skip_reason.as_deref().unwrap_or("ineligible")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
