use super::{
    desktop_ipc::DesktopIpc,
    ipc_call_error::IpcCallError,
    manifest_store::{deferred_account_binding, load_manifest, prune_ineligible_targets},
    pending_target::PendingTarget,
    queue_snapshot::pending_count,
    recovery_banner::RecoveryBanner,
    recovery_dispatch_identity_guard::RecoveryDispatchIdentityGuard,
    recovery_mode::RecoveryMode,
    recovery_service::recover_threads_with_banner,
    restart_checkpoint_service::{
        post_checkpoint_status_with_budget, POST_CHECKPOINT_SCAN_BUDGET_BYTES,
    },
    target_dispatch::{should_dispatch, should_resume_queued},
};
use crate::{storage, switcher, switcher::ThreadRolloutState};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use super::deferred_recovery_service::{
    probe_or_retry_navigation, record_navigation_attempt, DeferredNavigationRoute,
    NavigationAttempt,
};

const OWNER_MOUNT_WAIT: Duration = Duration::from_secs(90);

pub(super) struct DeferredMountBannerService;

impl DeferredMountBannerService {
    /// Returns true when this probe spent its navigation attempt on one task.
    /// The original checkpoint is left untouched until owner proof and the
    /// ordinary recovery dispatch gate have both succeeded.
    pub(super) fn try_mount(
        targets: &[PendingTarget],
        account_id: &str,
        route: DeferredNavigationRoute,
        navigation: &mut NavigationAttempt,
        desktop: &mut DesktopIpc,
    ) -> Result<bool, String> {
        let home = storage::codex_home();
        let mut budget = POST_CHECKPOINT_SCAN_BUDGET_BYTES;
        for item in targets.iter().filter(|item| {
            item.awaiting_owner && item.owner_account_id.as_deref() == Some(account_id)
        }) {
            let mut singleton = vec![item.clone()];
            prune_ineligible_targets(&home, &mut singleton)?;
            if singleton.is_empty() {
                continue;
            }
            let checkpoint = post_checkpoint_status_with_budget(&home, item, &mut budget);
            let pending = pending_count(&home, &item.id)?;
            let state = switcher::inspect_thread_rollout_state(&home, &item.id);
            if !eligible_mount(item, account_id, state, pending, checkpoint) {
                continue;
            }
            match desktop.discover_owner_info_once(&item.id) {
                Ok(_) => continue,
                Err(IpcCallError::NoClientFound) => {}
                Err(error) => return Err(error.to_string()),
            }
            Self::attempt(item, account_id, route, navigation, desktop)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn attempt(
        target: &PendingTarget,
        account_id: &str,
        route: DeferredNavigationRoute,
        navigation: &mut NavigationAttempt,
        desktop: &mut DesktopIpc,
    ) -> Result<(), String> {
        let ids = vec![target.id.clone()];
        let mode = if target.captured_restart {
            RecoveryMode::DeferredCaptured
        } else {
            RecoveryMode::DeferredOwned
        };
        let mut banner = RecoveryBanner::start_for_running_desktop(
            &super::automation_guard::operation_id_for_banner("deferred_owner_mount"),
            &ids,
            "thread_recovery",
        )?;
        let identity = RecoveryDispatchIdentityGuard::capture(&banner, mode)?;
        if identity.account_id() != account_id
            || deferred_account_binding().as_deref() != Some(account_id)
        {
            return Err("Deferred mount account changed before task navigation".into());
        }
        let deadline = Instant::now() + OWNER_MOUNT_WAIT;
        let mounted = mount_with_banner(
            &mut banner,
            |banner| {
                identity.verify().map_err(|error| error.to_string())?;
                if deferred_account_binding().as_deref() != Some(account_id) {
                    return Err("Deferred mount Desktop account changed".into());
                }
                if banner.has_visible_panel() {
                    banner.verify_panel_alive()?;
                }
                Ok(())
            },
            |banner| banner.try_show_pending_for_mount(&ids),
            || {
                let _ = probe_or_retry_navigation(
                    || Err(IpcCallError::NoClientFound),
                    || {
                        record_navigation_attempt(navigation, route, Instant::now());
                        match route {
                            DeferredNavigationRoute::Ordinary => {
                                switcher::retry_thread_link_in_background(&target.id)
                            }
                            DeferredNavigationRoute::PinnedNative => {
                                switcher::retry_thread_link_natively_in_background(&target.id)
                            }
                        }
                    },
                    true,
                )?;
                Ok(())
            },
            || match desktop.discover_owner_info_once(&target.id) {
                Ok(_) => Ok(Some(true)),
                Err(IpcCallError::NoClientFound) if Instant::now() < deadline => {
                    sleep(Duration::from_millis(200));
                    Ok(Some(false))
                }
                Err(IpcCallError::NoClientFound) => Ok(None),
                Err(error) => Err(error.to_string()),
            },
            |banner| {
                let refreshed = load_manifest()?.into_iter().find(|item| {
                    item.id == target.id
                        && item.awaiting_owner
                        && item.owner_account_id.as_deref() == Some(account_id)
                });
                let Some(refreshed) = refreshed else {
                    return Err("Deferred mount target changed before recovery".into());
                };
                let mut eligible = vec![refreshed.clone()];
                prune_ineligible_targets(&storage::codex_home(), &mut eligible)?;
                if eligible.is_empty() {
                    return Err("Deferred mount target is no longer eligible".into());
                }
                recover_threads_with_banner(&ids, mode, banner)
            },
        )?;
        if !mounted {
            crate::logger::log(
                "WARN",
                "RECOVERY",
                "DEFERRED_OWNER_MOUNT_TIMED_OUT; checkpoint retained",
            );
        }
        Ok(())
    }
}

pub(super) fn eligible_mount(
    target: &PendingTarget,
    account_id: &str,
    state: ThreadRolloutState,
    pending: usize,
    checkpoint: Option<(bool, bool)>,
) -> bool {
    if !target.awaiting_owner
        || target.owner_account_id.as_deref() != Some(account_id)
        || !matches!(checkpoint, Some((false, false)))
    {
        return false;
    }
    let mode = if target.captured_restart {
        RecoveryMode::DeferredCaptured
    } else {
        RecoveryMode::DeferredOwned
    };
    if pending == 0 {
        should_dispatch(state, pending, mode)
    } else {
        should_resume_queued(state, mode)
    }
}

pub(super) fn mount_with_banner<B>(
    banner: &mut B,
    mut verify: impl FnMut(&mut B) -> Result<(), String>,
    mut show_pending: impl FnMut(&mut B) -> Result<bool, String>,
    navigate: impl FnOnce() -> Result<(), String>,
    mut poll_owner: impl FnMut() -> Result<Option<bool>, String>,
    recover: impl FnOnce(&mut B) -> Result<(), String>,
) -> Result<bool, String> {
    verify(banner)?;
    let _ = show_pending(banner)?;
    navigate()?;
    loop {
        verify(banner)?;
        let visible = show_pending(banner)?;
        match poll_owner()? {
            Some(true) => {
                verify(banner)?;
                if !visible && !show_pending(banner)? {
                    return Err(
                        "Desktop has no visible window after owner mount; recovery deferred".into(),
                    );
                }
                recover(banner)?;
                return Ok(true);
            }
            Some(false) => continue,
            None => return Ok(false),
        }
    }
}

#[cfg(test)]
#[path = "deferred_mount_banner_service.test.rs"]
mod tests;
