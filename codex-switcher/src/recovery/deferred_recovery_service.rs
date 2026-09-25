use super::{
    automation_guard::operation_lock,
    desktop_ipc::DesktopIpc,
    ipc_call_error::IpcCallError,
    manifest_store::{
        current_account_binding, load_manifest, prune_ineligible_targets, write_manifest,
    },
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
    recovery_service::recover_threads,
};
use crate::{storage, switcher};
use std::{
    thread,
    time::{Duration, Instant},
};

const PROBE_INTERVAL: Duration = Duration::from_secs(15);
const NAVIGATION_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// Watches only targets that failed before an IPC dispatch because Desktop had
/// no owner. The daemon owns the probe; a short worker keeps quota polling live
/// while proof of recovered agent work is collected.
pub(crate) struct DeferredRecoveryService {
    worker: Option<thread::JoinHandle<()>>,
    last_probe: Option<Instant>,
    last_navigation: Option<Instant>,
}

impl DeferredRecoveryService {
    pub(crate) fn new() -> Self {
        Self {
            worker: None,
            last_probe: None,
            last_navigation: None,
        }
    }

    pub(crate) fn poll(&mut self) {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return;
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                crate::logger::log("ERROR", "RECOVERY", "DEFERRED_RECOVERY_WORKER_PANICKED");
            }
        }
        if self
            .last_probe
            .is_some_and(|at| at.elapsed() < PROBE_INTERVAL)
        {
            return;
        }
        self.last_probe = Some(Instant::now());
        let retry_navigation = should_retry_navigation(self.last_navigation, Instant::now());
        match thread::Builder::new()
            .name("codex-deferred-recovery".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                if let Err(error) = Self::run_once(retry_navigation) {
                    crate::logger::log(
                        "WARN",
                        "RECOVERY",
                        &format!(
                            "DEFERRED_RECOVERY_PROBE_FAILED reason={}",
                            super::recovery_service::sanitize_recovery_error(&error)
                        ),
                    );
                }
            }) {
            Ok(worker) => {
                self.worker = Some(worker);
                if retry_navigation {
                    self.last_navigation = Some(Instant::now());
                }
            }
            Err(error) => crate::logger::log(
                "WARN",
                "RECOVERY",
                &format!("DEFERRED_RECOVERY_WORKER_FAILED reason={error}"),
            ),
        }
    }

    fn run_once(retry_navigation: bool) -> Result<(), String> {
        let _operation = match operation_lock() {
            Ok(lock) => lock,
            Err(error) if error == "Another desktop switch/recovery is in progress" => {
                return Ok(())
            }
            Err(error) => return Err(error),
        };
        let home = storage::codex_home();
        let mut targets = load_manifest()?;
        let original_len = targets.len();
        prune_ineligible_targets(&home, &mut targets)?;
        if targets.len() != original_len {
            write_manifest(&targets)?;
        }
        if !targets.iter().any(|target| target.awaiting_owner) {
            return Ok(());
        }
        let Some(account_id) = current_account_binding() else {
            return Ok(());
        };
        let mut desktop = DesktopIpc::connect(Duration::from_secs(2))?;
        let ready = select_ready_targets(&targets, &account_id, |id| {
            probe_or_retry_navigation(
                || desktop.discover_owner_info_once(id).map(|_| ()),
                || switcher::retry_thread_link_in_background(id),
                retry_navigation,
            )
        })?;
        drop(desktop);
        for id in ready {
            // The lock is held throughout. A previous target's recovery may
            // have pruned this target or made it ineligible.
            let Some(target) = load_manifest()?.into_iter().find(|target| {
                target.id == id
                    && target.awaiting_owner
                    && target.owner_account_id.as_deref() == Some(account_id.as_str())
            }) else {
                continue;
            };
            let mode = if target.captured_restart {
                RecoveryMode::DeferredCaptured
            } else {
                RecoveryMode::DeferredOwned
            };
            if let Err(error) = recover_threads(std::slice::from_ref(&id), mode) {
                crate::logger::log(
                    "WARN",
                    "RECOVERY",
                    &format!(
                        "DEFERRED_RECOVERY_UNVERIFIED thread={id} reason={}",
                        super::recovery_service::sanitize_recovery_error(&error)
                    ),
                );
            }
        }
        Ok(())
    }
}

pub(super) fn should_retry_navigation(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= NAVIGATION_RETRY_INTERVAL)
}

pub(super) fn probe_or_retry_navigation(
    discover: impl FnOnce() -> Result<(), IpcCallError>,
    navigate: impl FnOnce() -> Result<(), String>,
    retry_navigation: bool,
) -> Result<bool, String> {
    match discover() {
        Ok(()) => Ok(true),
        Err(IpcCallError::NoClientFound) => {
            if retry_navigation {
                navigate()?;
            }
            Ok(false)
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn select_ready_targets(
    targets: &[PendingTarget],
    account_id: &str,
    mut has_owner: impl FnMut(&str) -> Result<bool, String>,
) -> Result<Vec<String>, String> {
    let mut ready = Vec::new();
    for target in targets.iter().filter(|target| {
        target.awaiting_owner && target.owner_account_id.as_deref() == Some(account_id)
    }) {
        if has_owner(&target.id)? {
            ready.push(target.id.clone());
        }
    }
    Ok(ready)
}
