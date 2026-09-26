#[path = "switcher/account_switch_noop_service.rs"]
mod account_switch_noop_service;
#[path = "switcher/account_switch_service.rs"]
mod account_switch_service;
#[path = "switcher/account_target_resolver.rs"]
mod account_target_resolver;
#[path = "switcher/codex_app_lifecycle.rs"]
mod codex_app_lifecycle;
#[path = "switcher/codex_availability_service.rs"]
mod codex_availability_service;
#[path = "switcher/desktop_session_binding_service.rs"]
mod desktop_session_binding_service;
#[path = "switcher/recovery_command_service.rs"]
mod recovery_command_service;
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
pub use account_target_resolver::resolve_target_account_idx;
pub(crate) use codex_app_lifecycle::{
    current_codex_app_pids, launch_codex_app, stop_codex_app_gracefully,
};
pub use codex_app_lifecycle::{is_codex_app_running, send_macos_notification};
pub use recovery_command_service::{
    dispatch_self_restart, restart_and_recover, resume_thread_interactive,
};
pub use switch_outcome::SwitchOutcome;
pub use switch_trigger::SwitchTrigger;
pub use thread_detection_service::{
    detect_in_progress_threads, detect_quota_blocked_user_threads_since,
    detect_recent_quota_blocked_user_threads,
};
pub(crate) use thread_identity::retry_thread_link_in_background;
pub use thread_identity::{
    clean_thread_id, get_most_recent_threads, is_user_thread, open_thread_in_codex,
};
pub use thread_rollout_inspector::{
    find_thread_rollout_path, inspect_thread_rollout_state, RECENT_QUOTA_WINDOW_SECS,
};
pub use thread_rollout_state::ThreadRolloutState;

#[cfg(test)]
use account_switch_service::{prioritize_primary, prioritize_primary_if_user};
#[cfg(test)]
use codex_app_lifecycle::{parse_codex_app_pids, wait_for_app_exit_with, CODEX_APP_EXECUTABLE};
#[cfg(test)]
use recovery_command_service::has_codex_ancestor;
#[cfg(test)]
use thread_detection_service::append_eligible_pending;
#[cfg(test)]
use thread_rollout_inspector::inspect_thread_rollout_state_from_lines;

#[cfg(test)]
#[path = "switcher.test.rs"]
mod tests;
