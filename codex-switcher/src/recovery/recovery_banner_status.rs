use super::recovery_banner::RecoveryBanner;
use crate::recovery_banner::BannerSessionStatus;

impl RecoveryBanner {
    pub(crate) fn record_status(&mut self, id: &str, status: BannerSessionStatus) {
        if !self.has_visible_panel() {
            self.pending_statuses.push((id.to_owned(), status));
            return;
        }
        if let Err(error) = self.update_status(id, status) {
            crate::logger::log(
                "WARN",
                "RECOVERY",
                &format!(
                    "RECOVERY_BANNER_STATUS_FAILED status={status:?} reason={}",
                    super::recovery_service::sanitize_recovery_error(&error)
                ),
            );
        }
    }

    pub(crate) fn replay_pending_statuses_into(&mut self, next: &mut Self) -> Result<(), String> {
        if !next.has_visible_panel() {
            next.pending_statuses.append(&mut self.pending_statuses);
            return Ok(());
        }
        for (id, status) in &self.pending_statuses {
            next.update_status(id, *status)?;
        }
        self.pending_statuses.clear();
        Ok(())
    }
}

#[cfg(test)]
#[path = "recovery_banner_status.test.rs"]
mod tests;
