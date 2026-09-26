//! Recovery is successful only when the target rollout records new agent work.
//! IPC dispatch, task_started, or a queue acknowledgement is not proof.
mod active_auth_binding_service;
mod automation_guard;
mod checkpoint_confirmation;
mod checkpoint_scan_cache;
mod deferred_recovery_service;
mod desktop_account_binding_service;
mod desktop_ipc;
mod dispatch_mark_error;
mod evidence;
mod foreground_checkpoint_service;
mod ipc_call_error;
mod ipc_protocol;
mod ipc_read_error;
mod ipc_socket;
mod manifest_prune_service;
mod manifest_store;
mod observer;
mod owner_info;
mod pending_manifest;
mod pending_target;
mod queue_snapshot;
mod recovery_banner;
mod recovery_banner_status;
mod recovery_checkpoint;
mod recovery_dispatch_checkpoint_service;
mod recovery_dispatch_identity_guard;
mod recovery_error_sanitizer;
mod recovery_manifest_snapshot;
mod recovery_mode;
mod recovery_service;
mod recovery_target;
mod restart_checkpoint_service;
mod running_desktop_banner;
mod stored_manifest;
mod target_dispatch;
mod thread_identity;
mod thread_index_service;
mod window_capture;
mod window_restore;

pub(crate) use automation_guard::{
    arm_automation_cooldown, automation_cooldown_remaining, clear_restart_cancellation,
    operation_id_for_banner, restart_cancellation_requested,
};
pub use automation_guard::{claim_restart_operation, operation_lock};
pub(crate) use deferred_recovery_service::DeferredRecoveryService;
use desktop_ipc::DesktopIpc;
pub use manifest_store::{load_ownerless_pending, load_pending};
pub(crate) use recovery_banner::RecoveryBanner;
pub(crate) use recovery_manifest_snapshot::RecoveryManifestSnapshot;
pub(crate) use recovery_mode::RecoveryMode;
pub use recovery_service::recover_threads;
pub(crate) use recovery_service::recover_threads_with_banner;
pub use restart_checkpoint_service::save_pending;
use std::time::Duration;
pub(crate) use window_capture::save_desktop_window_bounds;
pub use window_capture::{
    get_active_desktop_window_bounds, get_saved_desktop_window_bounds,
    should_preserve_window_bounds,
};
pub(crate) use window_restore::{restore_desktop_window_bounds, verify_desktop_stable};
/// Fail closed before stopping Codex by handshaking its same-user IPC router.
pub(crate) fn preflight_desktop_dispatch() -> Result<(), String> {
    let client = DesktopIpc::connect_with_retry(Duration::from_secs(30))?;
    drop(client);
    crate::runtime_print!("RECOVERY_CHANNEL_CONFIRMED transport=desktop_ipc");
    Ok(())
}

#[cfg(test)]
#[path = "recovery/automation_guard.test.rs"]
mod automation_guard_tests;
#[cfg(test)]
#[path = "recovery/deferred_recovery.test.rs"]
mod deferred_recovery_tests;
#[cfg(test)]
#[path = "recovery/evidence.test.rs"]
mod evidence_tests;
#[cfg(test)]
#[path = "recovery/foreground_checkpoint_service.test.rs"]
mod foreground_checkpoint_service_tests;
#[cfg(test)]
#[path = "recovery/ipc_protocol.test.rs"]
mod ipc_protocol_tests;
#[cfg(test)]
#[path = "recovery/manifest_store.test.rs"]
mod manifest_store_tests;
#[cfg(test)]
#[path = "recovery/observer.test.rs"]
mod observer_tests;
#[cfg(test)]
#[path = "recovery/queue_snapshot.test.rs"]
mod queue_snapshot_tests;
#[cfg(test)]
#[path = "recovery/recovery_checkpoint.test.rs"]
mod recovery_checkpoint_tests;
#[cfg(test)]
#[path = "recovery/recovery_manifest_snapshot.test.rs"]
mod recovery_manifest_snapshot_tests;
#[cfg(test)]
#[path = "recovery/recovery_service.test.rs"]
mod recovery_service_tests;
#[cfg(test)]
#[path = "recovery/target_dispatch.test.rs"]
mod target_dispatch_tests;
#[cfg(test)]
#[path = "recovery/thread_identity.test.rs"]
mod thread_identity_tests;
#[cfg(test)]
#[path = "recovery/window_capture.test.rs"]
mod window_capture_tests;
