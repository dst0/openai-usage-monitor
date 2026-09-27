use super::reset_journal::ResetJournal;
use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_policy::{now_string, unresolved_attempt};

/// Marks an automatic attempt `pending` immediately before its request.
///
/// If the marker cannot be saved durably, no request has been sent. A journal
/// that reads back exactly equal to the marker (its key is fresh or was never
/// sent, and its timestamp is new) is provably this write's, so it is
/// withdrawn to a retryable `journal_error` that keeps the key and task: an
/// unsent attempt must never suppress rotation or block manual and other
/// accounts' resets. An unreadable or different journal may describe a request
/// that did leave and is left unchanged.
pub(super) struct ResetPendingMarkService<'s, 'f> {
    store: &'s ResetJournalStore<'f>,
}

impl<'s, 'f> ResetPendingMarkService<'s, 'f> {
    pub(super) fn new(store: &'s ResetJournalStore<'f>) -> Self {
        Self { store }
    }

    pub(super) fn mark(&self, journal: &mut ResetJournal) -> Result<(), String> {
        journal.state = "pending".into();
        journal.reason = None;
        journal.updated_at = Some(now_string());
        let Err(error) = self.store.write(journal) else {
            return Ok(());
        };
        Err(format!(
            "{error}; no reset request was sent, and {}",
            self.withdraw_unsent(journal)
        ))
    }

    fn withdraw_unsent(&self, marker: &ResetJournal) -> String {
        match self.store.load() {
            Ok(saved) if saved == *marker => {
                let withdrawn = ResetJournal {
                    state: "journal_error".into(),
                    reason: Some("pending_marker_not_durable".into()),
                    updated_at: Some(now_string()),
                    ..saved
                };
                match self.store.write(&withdrawn) {
                    Ok(()) => "the unsent attempt was withdrawn".into(),
                    Err(_) => "the unsent attempt could not be durably withdrawn and may still \
                               show pending until a same-key retry or reconciliation settles it"
                        .into(),
                }
            }
            Ok(saved) if unresolved_attempt(&saved) => {
                "a different unresolved attempt was left unchanged".into()
            }
            Ok(_) => "no unresolved attempt is recorded".into(),
            Err(_) => "the journal could not be read and was left unchanged".into(),
        }
    }
}
