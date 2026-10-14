use super::ConfigChanges;
use crate::models::Settings;

/// Prints each setting a `cxi config` call changed, as saved.
pub(super) fn print_saved(changes: &ConfigChanges, saved: &Settings) {
    let flags = [
        (
            "restart_app_on_switch",
            changes.restart_app_on_switch,
            saved.restart_app_on_switch,
        ),
        (
            "auto_switch_enabled",
            changes.auto_switch_enabled,
            saved.auto_switch_enabled,
        ),
        (
            "auto_switch_business_only",
            changes.auto_switch_business_only,
            saved.auto_switch_business_only,
        ),
        (
            "auto_switch_business_priority",
            changes.auto_switch_business_priority,
            saved.auto_switch_business_priority,
        ),
        (
            "preserve_window_bounds_on_restart",
            changes.preserve_window_bounds,
            saved.preserve_window_bounds_on_restart,
        ),
        (
            "session_compression_enabled",
            changes.session_compression_enabled,
            saved.session_compression_enabled,
        ),
    ];
    for (name, _, value) in flags.iter().filter(|(_, given, _)| given.is_some()) {
        println!("✅ Setting updated: {} = {}", name, value);
    }
    if changes.session_compression_min_age_days.is_some() {
        println!(
            "✅ Setting updated: session_compression_min_age_days = {}",
            saved.session_compression_min_age_days
        );
    }
    if changes.changes_weekly_reset() {
        println!(
            "✅ Setting updated: auto_reset_weekly_enabled = {}, auto_reset_weekly_min_remaining_seconds = {}",
            saved.auto_reset_weekly_enabled, saved.auto_reset_weekly_min_remaining_seconds
        );
    }
}
