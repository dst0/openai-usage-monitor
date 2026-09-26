use super::{
    automation_guard::operation_lock,
    desktop_ipc::DesktopIpc,
    ipc_call_error::IpcCallError,
    manifest_store::{
        load_manifest, prune_ineligible_targets, recovery_account_binding, write_manifest,
    },
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
    recovery_service::recover_threads,
    restart_checkpoint_service::{
        post_checkpoint_status_with_budget, POST_CHECKPOINT_SCAN_BUDGET_BYTES,
    },
};
use crate::{storage, switcher};
use std::{
    thread,
    time::{Duration, Instant},
};

const PROBE_INTERVAL: Duration = Duration::from_secs(15);
const NAVIGATION_RETRY_INTERVAL: Duration = Duration::from_secs(60);
type NavigationAttempt = Option<(Instant, DeferredNavigationRoute)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeferredNavigationRoute {
    Ordinary,
    PinnedNative,
}

impl DeferredNavigationRoute {
    pub(super) fn after(previous: Option<Self>) -> Self {
        match previous {
            Some(Self::Ordinary) => Self::PinnedNative,
            Some(Self::PinnedNative) | None => Self::Ordinary,
        }
    }
}

/// Watches only targets that failed before an IPC dispatch because Desktop had
/// no owner. The daemon owns the probe; a short worker keeps quota polling live
/// while proof of recovered agent work is collected.
pub(crate) struct DeferredRecoveryService {
    worker: Option<thread::JoinHandle<NavigationAttempt>>,
    last_probe: Option<Instant>,
    last_navigation: Option<Instant>,
    last_attempt_route: Option<DeferredNavigationRoute>,
}

impl DeferredRecoveryService {
    pub(crate) fn new() -> Self {
        Self {
            worker: None,
            last_probe: None,
            last_navigation: None,
            last_attempt_route: None,
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
            match worker.join() {
                Ok(Some((at, route))) => {
                    self.last_attempt_route = Some(route);
                    self.last_navigation = Some(at);
                }
                Ok(None) => {}
                Err(_) => {
                    crate::logger::log("ERROR", "RECOVERY", "DEFERRED_RECOVERY_WORKER_PANICKED")
                }
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
        let route = DeferredNavigationRoute::after(self.last_attempt_route);
        match thread::Builder::new()
            .name("codex-deferred-recovery".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let (result, navigation) =
                    tracked_probe(|navigation| Self::run_once(retry_navigation, route, navigation));
                if let Err(error) = result {
                    crate::logger::log(
                        "WARN",
                        "RECOVERY",
                        &format!(
                            "DEFERRED_RECOVERY_PROBE_FAILED reason={}",
                            super::recovery_error_sanitizer::RecoveryErrorSanitizer::sanitize(
                                &error
                            )
                        ),
                    );
                }
                navigation
            }) {
            Ok(worker) => {
                self.worker = Some(worker);
            }
            Err(error) => crate::logger::log(
                "WARN",
                "RECOVERY",
                &format!("DEFERRED_RECOVERY_WORKER_FAILED reason={error}"),
            ),
        }
    }

    fn run_once(
        retry_navigation: bool,
        route: DeferredNavigationRoute,
        navigation: &mut NavigationAttempt,
    ) -> Result<(), String> {
        let _operation = match operation_lock() {
            Ok(lock) => lock,
            Err(error) if error == "Another desktop switch/recovery is in progress" => {
                return Ok(())
            }
            Err(error) => return Err(error),
        };
        let home = storage::codex_home();
        let targets = load_manifest()?;
        if !targets.iter().any(|target| target.awaiting_owner) {
            return Ok(());
        }
        let Some(account_id) = recovery_account_binding(true) else {
            return Ok(());
        };
        let mut desktop = DesktopIpc::connect(Duration::from_secs(2))?;
        let ready = select_scanned_ready_targets(
            &targets,
            &account_id,
            |id| {
                probe_or_retry_navigation(
                    || desktop.discover_owner_info_once(id).map(|_| ()),
                    || {
                        let result = match route {
                            DeferredNavigationRoute::Ordinary => {
                                switcher::retry_thread_link_in_background(id)
                            }
                            DeferredNavigationRoute::PinnedNative => {
                                switcher::retry_thread_link_natively_in_background(id)
                            }
                        };
                        record_navigation_attempt(navigation, route, Instant::now());
                        result
                    },
                    retry_navigation,
                )
            },
            |target, budget| Ok(post_checkpoint_status_with_budget(&home, target, budget).is_some()),
        )?;
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
            let mut eligible = vec![target.clone()];
            prune_ineligible_targets(&home, &mut eligible)?;
            if eligible.is_empty() {
                let mut manifest = load_manifest()?;
                manifest.retain(|item| item.id != id);
                write_manifest(&manifest)?;
                continue;
            }
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
                        super::recovery_error_sanitizer::RecoveryErrorSanitizer::sanitize(&error)
                    ),
                );
            }
        }
        Ok(())
    }
}

pub(super) fn tracked_probe(
    work: impl FnOnce(&mut NavigationAttempt) -> Result<(), String>,
) -> (Result<(), String>, NavigationAttempt) {
    let mut navigation = None;
    let result = work(&mut navigation);
    (result, navigation)
}

pub(super) fn record_navigation_attempt(
    first: &mut NavigationAttempt,
    route: DeferredNavigationRoute,
    at: Instant,
) {
    if first.is_none() {
        *first = Some((at, route));
    }
}

pub(super) fn select_scanned_ready_targets(
    targets: &[PendingTarget],
    account_id: &str,
    has_owner: impl FnMut(&str) -> Result<bool, String>,
    mut scan_complete: impl FnMut(&PendingTarget, &mut u64) -> Result<bool, String>,
) -> Result<Vec<String>, String> {
    let ready = select_ready_targets(targets, account_id, has_owner)?;
    if ready.is_empty() {
        return Ok(ready);
    }
    let per_target_budget = POST_CHECKPOINT_SCAN_BUDGET_BYTES / ready.len() as u64;
    let mut scanned = Vec::new();
    for id in ready {
        let Some(target) = targets.iter().find(|target| target.id == id) else {
            continue;
        };
        let mut budget = per_target_budget;
        if scan_complete(target, &mut budget)? {
            scanned.push(id);
        }
    }
    Ok(scanned)
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
                if let Err(error) = navigate() {
                    if switcher::is_fatal_thread_navigation_error(&error) {
                        return Err(error);
                    }
                    crate::logger::log(
                        "WARN",
                        "RECOVERY",
                        "DEFERRED_TASK_NAVIGATION_FAILED; owner probe continues",
                    );
                }
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
