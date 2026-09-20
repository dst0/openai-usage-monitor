use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_outcome::DistributionOutcome;
use super::distribution_request::DistributionRequest;

pub trait DistributionExecutor {
    fn execute(&self, request: DistributionRequest) -> Result<DistributionOutcome, String>;
}

impl DistributionExecutor for DistributionCoordinator {
    fn execute(&self, request: DistributionRequest) -> Result<DistributionOutcome, String> {
        DistributionCoordinator::execute(self, request)
    }
}
