use super::{
    manifest_store::validate_target_account_binding, pending_target::PendingTarget,
    recovery_mode::RecoveryMode, thread_identity::valid_id,
};
use crate::switcher;
use std::path::Path;

/// Journal a checkpoint for every requested target before any Desktop request.
/// Returns this operation's checkpoint view and the explicitly claimed IDs.
pub(super) fn checkpoint_targets(
    home: &Path,
    manifest: &mut Vec<PendingTarget>,
    ids: &[String],
    mode: RecoveryMode,
    binding: Option<&str>,
) -> Result<(Vec<PendingTarget>, Vec<String>), String> {
    checkpoint_targets_with(manifest, ids, mode, binding, |id| {
        switcher::find_thread_rollout_path(home, id)
            .and_then(|path| path.metadata().ok().map(|metadata| metadata.len()))
    })
}

/// An explicit `cxi resume <id>` is a new user-authorized operation, so a
/// deferred owner wait left by an earlier automatic run must not veto it. The
/// claim lives only in the returned view: the journal keeps the original
/// binding and checkpoint, so a crash or a failure before IPC dispatch leaves
/// the deferred retry exactly as it was.
pub(super) fn checkpoint_targets_with(
    manifest: &mut Vec<PendingTarget>,
    ids: &[String],
    mode: RecoveryMode,
    binding: Option<&str>,
    rollout_len: impl Fn(&str) -> Option<u64>,
) -> Result<(Vec<PendingTarget>, Vec<String>), String> {
    if ids.iter().any(|id| !valid_id(id)) {
        return Err("Invalid thread ID".into());
    }
    let claimed: Vec<String> = if mode == RecoveryMode::ExplicitTarget {
        ids.iter()
            .filter(|id| {
                manifest
                    .iter()
                    .any(|target| target.id == **id && target.awaiting_owner)
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let bound: Vec<String> = ids
        .iter()
        .filter(|id| !claimed.contains(id))
        .cloned()
        .collect();
    validate_target_account_binding(manifest, &bound, binding)?;
    for id in ids {
        // `save_pending` already captured pre-restart offsets. Recovery-only
        // calls reach this path without that earlier phase, so checkpoint them
        // here before any Desktop request is sent.
        let offset = rollout_len(id);
        if let Some(target) = manifest.iter_mut().find(|target| target.id == *id) {
            // A recovery-only request is a new operation and must never reuse a
            // stale offset left by an earlier failed restart. A captured restart
            // intentionally retains its pre-shutdown checkpoint.
            if !mode.preserves_checkpoint() && !target.awaiting_owner {
                target.offset = offset;
            }
        } else {
            manifest.push(PendingTarget {
                id: id.clone(),
                offset,
                awaiting_owner: false,
                captured_restart: mode == RecoveryMode::CapturedRestart,
                owner_account_id: None,
            });
        }
    }
    let mut view = manifest.clone();
    for target in view
        .iter_mut()
        .filter(|target| claimed.contains(&target.id))
    {
        target.awaiting_owner = false;
        target.owner_account_id = None;
        target.offset = rollout_len(&target.id);
        crate::runtime_print!(
            "RECOVERY_DEFERRED_TARGET_CLAIMED thread={} reason=explicit_request account_verified={}",
            target.id,
            binding.is_some()
        );
    }
    Ok((view, claimed))
}
