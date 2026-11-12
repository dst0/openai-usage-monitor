use super::{
    manifest_store::{load_manifest, prune_ineligible_targets, write_manifest},
    pending_target::PendingTarget,
};
use std::path::Path;

pub(super) struct DeferredManifestFilter;

impl DeferredManifestFilter {
    /// Inspects and prunes stale or ineligible ownerless targets from the recovery manifest
    /// before any owner discovery wait is started.
    pub(super) fn prune_and_load_active(home: &Path) -> Result<Vec<PendingTarget>, String> {
        let targets = load_manifest()?;
        if !targets.iter().any(|target| target.awaiting_owner) {
            return Ok(Vec::new());
        }
        let initial_count = targets.len();
        let mut retained = Vec::new();
        for target in &targets {
            if !target.awaiting_owner {
                retained.push(target.clone());
                continue;
            }
            let mut single = vec![target.clone()];
            prune_ineligible_targets(home, &mut single)?;
            if !single.is_empty() {
                retained.push(target.clone());
            }
        }
        if retained.len() != initial_count {
            write_manifest(&retained)?;
        }
        Ok(retained)
    }
}

#[cfg(test)]
#[path = "deferred_manifest_filter.test.rs"]
mod tests;
