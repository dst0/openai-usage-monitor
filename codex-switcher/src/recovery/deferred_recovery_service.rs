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
use crate::storage;
use std::{
    thread,
    time::{Duration, Instant},
};

const PROBE_INTERVAL: Duration = Duration::from_secs(15);

/// Watches only targets that failed before an IPC dispatch because Desktop had
/// no owner. The daemon owns the probe; a short worker keeps quota polling live
/// while proof of recovered agent work is collected.
pub(crate) struct DeferredRecoveryService {
    worker: Option<thread::JoinHandle<()>>,
    last_probe: Option<Instant>,
}

impl DeferredRecoveryService {
    pub(crate) fn new() -> Self {
        Self {
            worker: None,
            last_probe: None,
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
        match thread::Builder::new()
            .name("codex-deferred-recovery".into())
            .stack_size(512 * 1024)
            .spawn(|| {
                if let Err(error) = Self::run_once() {
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
            Ok(worker) => self.worker = Some(worker),
            Err(error) => crate::logger::log(
                "WARN",
                "RECOVERY",
                &format!("DEFERRED_RECOVERY_WORKER_FAILED reason={error}"),
            ),
        }
    }

    fn run_once() -> Result<(), String> {
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
            match desktop.discover_owner_info_once(id) {
                Ok(_) => Ok(true),
                Err(IpcCallError::NoClientFound) => Ok(false),
                Err(error) => Err(error.to_string()),
            }
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
            if let Err(error) = recover_threads(&[id.clone()], mode) {
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
