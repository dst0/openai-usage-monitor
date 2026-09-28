use super::{pending_target::PendingTarget, thread_identity::valid_id};
use std::collections::HashSet;

pub(super) fn valid_unique_targets(targets: &[PendingTarget]) -> bool {
    let mut ids = HashSet::with_capacity(targets.len());
    targets.iter().all(|target| {
        valid_id(&target.id)
            && ids.insert(target.id.as_str())
            && target.auth_rotation.as_ref().is_none_or(|evidence| {
                let source = &evidence.source_account_id;
                let destination = &evidence.target_account_id;
                !source.is_empty()
                    && source.len() <= 256
                    && !destination.is_empty()
                    && destination.len() <= 256
                    && source != destination
                    && !evidence.turn_id.is_empty()
                    && evidence.turn_id.len() <= 128
                    && evidence.rollout_dev != 0
                    && evidence.rollout_ino != 0
                    && (evidence.queue_snapshot.database_identity.is_none()
                        || evidence
                            .queue_snapshot
                            .database_identity
                            .is_some_and(|(dev, ino)| dev != 0 && ino != 0))
                    && evidence.queue_snapshot.pending <= 1_000_000
                    && target.captured_restart
                    && target.offset.is_some()
            })
    })
}
