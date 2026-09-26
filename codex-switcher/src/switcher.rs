#[path = "switcher/account_switch_auth_service.rs"]
mod account_switch_auth_service;
#[path = "switcher/account_switch_commit_service.rs"]
mod account_switch_commit_service;
#[path = "switcher/account_switch_noop_service.rs"]
mod account_switch_noop_service;
#[path = "switcher/account_switch_preflight_service.rs"]
mod account_switch_preflight_service;
#[path = "switcher/account_switch_service.rs"]
mod account_switch_service;
#[path = "switcher/account_target_resolver.rs"]
mod account_target_resolver;
#[path = "switcher/active_auth_registry_sync_service.rs"]
mod active_auth_registry_sync_service;
#[path = "switcher/codex_app_lifecycle.rs"]
mod codex_app_lifecycle;
#[path = "switcher/codex_availability_service.rs"]
mod codex_availability_service;
#[path = "switcher/codex_process_probe.rs"]
mod codex_process_probe;
#[path = "switcher/desktop_session_binding_service.rs"]
mod desktop_session_binding_service;
#[path = "switcher/desktop_writer_exit_gate.rs"]
mod desktop_writer_exit_gate;
#[path = "switcher/direct_switch_journal.rs"]
mod direct_switch_journal;
#[cfg(target_os = "macos")]
#[path = "switcher/pinned_thread_link_launch_spec.rs"]
mod pinned_thread_link_launch_spec;
#[cfg(not(target_os = "macos"))]
#[path = "switcher/pinned_thread_link_unsupported.rs"]
mod pinned_thread_link_launch_spec;
#[path = "switcher/primary_target_selection.rs"]
mod primary_target_selection;
#[path = "switcher/recovery_command_service.rs"]
mod recovery_command_service;
#[path = "switcher/restart_worker_dispatch_service.rs"]
mod restart_worker_dispatch_service;
#[path = "switcher/switch_outcome.rs"]
mod switch_outcome;
#[path = "switcher/switch_trigger.rs"]
mod switch_trigger;
#[path = "switcher/thread_detection_service.rs"]
mod thread_detection_service;
#[path = "switcher/thread_identity.rs"]
mod thread_identity;
#[path = "switcher/thread_rollout_inspector.rs"]
mod thread_rollout_inspector;
#[path = "switcher/thread_rollout_state.rs"]
mod thread_rollout_state;

pub use account_switch_service::switch_to_account;
#[cfg(test)]
use account_switch_service::switch_to_account_with;
pub use account_target_resolver::resolve_target_account_idx;
pub(crate) use active_auth_registry_sync_service::ActiveAuthRegistrySyncService;
pub use codex_app_lifecycle::send_macos_notification;
pub(crate) use codex_app_lifecycle::{
    current_codex_app_pids, current_codex_app_pids_checked, is_codex_app_running_checked,
    is_shared_auth_active_checked, launch_codex_app, preflight_shutdown_windows,
    stop_codex_app_gracefully,
};
#[cfg(test)]
pub(crate) use direct_switch_journal::create_direct_switch_intent_for_test;
pub(crate) use direct_switch_journal::reconcile_pending_direct_switch;
pub(crate) use pinned_thread_link_launch_spec::is_identity_change as is_fatal_thread_navigation_error;
use primary_target_selection::prioritize_primary_if_user;
pub use recovery_command_service::{
    dispatch_self_restart, restart_and_recover, resume_thread_interactive,
};
pub use switch_outcome::SwitchOutcome;
pub use switch_trigger::SwitchTrigger;
pub(crate) use thread_detection_service::quota_failure_timestamp;
pub use thread_detection_service::{
    detect_in_progress_threads, detect_quota_blocked_user_threads_since,
    detect_recent_quota_blocked_user_threads,
};
pub use thread_identity::{
    clean_thread_id, get_most_recent_threads, is_user_thread, open_thread_in_codex,
};
pub(crate) use thread_identity::{
    retry_thread_link_in_background, retry_thread_link_natively_in_background,
};
pub use thread_rollout_inspector::{
    find_thread_rollout_path, inspect_thread_rollout_state, RECENT_QUOTA_WINDOW_SECS,
};
pub use thread_rollout_state::ThreadRolloutState;

#[cfg(test)]
use codex_app_lifecycle::wait_for_app_exit_with;
#[cfg(test)]
use codex_process_probe::{parse_codex_app_pids, parse_shared_auth_activity, CODEX_APP_EXECUTABLE};
#[cfg(test)]
use primary_target_selection::prioritize_primary;
#[cfg(test)]
use recovery_command_service::has_codex_ancestor;
#[cfg(test)]
use thread_detection_service::append_eligible_pending;
#[cfg(test)]
use thread_rollout_inspector::inspect_thread_rollout_state_from_lines;

#[cfg(test)]
#[path = "switcher.test.rs"]
mod tests;
