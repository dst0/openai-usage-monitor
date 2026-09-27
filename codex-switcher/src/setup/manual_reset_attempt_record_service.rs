use super::manual_reset_attempt::ManualResetAttempt;
use super::manual_reset_attempt_store::ManualResetAttemptStore;

const RECONCILE: &str = "reconcile it before requesting another credit";

/// Records a manual reset attempt as `pending` immediately before its request
/// and proves that the saved record is this attempt's.
///
/// When saving or the readback fails, no request has been sent. The attempt
/// carries a fresh random idempotency key, so only a record that reads back
/// exactly equal to it is provably this command's; that record is withdrawn
/// (marked resolved) so an unsent attempt cannot block every later manual and
/// automatic reset. An unreadable or different record may belong to another
/// operation and is left for reconciliation.
pub(super) struct ManualResetAttemptRecordService<'s, 'f> {
    store: &'s ManualResetAttemptStore<'f>,
}

impl<'s, 'f> ManualResetAttemptRecordService<'s, 'f> {
    pub(super) fn new(store: &'s ManualResetAttemptStore<'f>) -> Self {
        Self { store }
    }

    pub(super) fn record_pending(&self, attempt: &ManualResetAttempt) -> Result<(), String> {
        let failure = match self.store.write(attempt) {
            Err(error) => error,
            Ok(()) => match self.store.load() {
                Ok(Some(saved)) if saved == *attempt => return Ok(()),
                Ok(_) => "Manual reset attempt readback did not match".to_string(),
                Err(error) => format!("Manual reset attempt readback failed ({error})"),
            },
        };
        Err(format!(
            "{failure}; no reset request was sent, and {}",
            self.withdraw_unsent(attempt)
        ))
    }

    fn withdraw_unsent(&self, attempt: &ManualResetAttempt) -> String {
        match self.store.load() {
            Ok(Some(saved)) if saved == *attempt => {
                let mut withdrawn = saved;
                withdrawn.mark_resolved();
                match self.store.write(&withdrawn) {
                    Ok(()) => "the unsent attempt was withdrawn".into(),
                    Err(_) => format!(
                        "the unsent attempt could not be durably withdrawn; if it still shows \
                         pending, {RECONCILE}"
                    ),
                }
            }
            Ok(Some(saved)) if saved.is_unresolved() => {
                format!("a different unresolved attempt was left unchanged; {RECONCILE}")
            }
            Ok(_) => "no unresolved attempt is recorded".into(),
            Err(_) => {
                format!(
                    "the recorded attempt could not be read and was left unchanged; {RECONCILE}"
                )
            }
        }
    }
}
