#[path = "setup/account_configuration.rs"]
mod account_configuration;
#[path = "setup/account_deduplication.rs"]
mod account_deduplication;
#[path = "setup/account_identity.rs"]
mod account_identity;
#[path = "setup/account_registration.rs"]
mod account_registration;
#[path = "setup/account_reset_service.rs"]
mod account_reset_service;
#[path = "setup/interactive_setup.rs"]
mod interactive_setup;
#[path = "setup/relogin_commit_service.rs"]
mod relogin_commit_service;
#[path = "setup/relogin_registry_commit_service.rs"]
mod relogin_registry_commit_service;
#[path = "setup/relogin_service.rs"]
mod relogin_service;
#[path = "setup/relogin_temp_home.rs"]
mod relogin_temp_home;

pub use account_configuration::{
    rename_account, reset_account_multiplier, set_account_multiplier, set_config_auto_reset_weekly,
    set_config_auto_switch_business_only, set_config_auto_switch_business_priority,
    set_config_auto_switch_enabled, set_config_preserve_window_bounds,
    set_config_restart_app_on_switch,
};
pub use account_deduplication::deduplicate_accounts_file;
pub use account_identity::{build_predictable_account_id, find_existing_account_idx};
pub use account_registration::{
    add_account_from_tokens, add_account_to_accounts_file, remove_account, save_current_as,
};
pub use account_reset_service::reset_account;
pub(crate) use account_reset_service::unresolved_manual_reset;
pub use interactive_setup::{login_and_add_account, resolve_codex_bin, run_interactive_setup};
pub use relogin_service::relogin_account;

#[cfg(test)]
use account_configuration::rename_account_with;
#[cfg(test)]
use account_deduplication::deduplicate_accounts;
#[cfg(test)]
use account_identity::find_existing_account_idx_from_parts;
#[cfg(test)]
use account_reset_service::reset_account_in_file;
#[cfg(test)]
use relogin_service::apply_relogin_to_accounts_file;

#[cfg(test)]
#[path = "setup/relogin_commit_service.test.rs"]
mod relogin_commit_tests;
#[cfg(test)]
#[path = "setup/relogin_registry_commit_service.test.rs"]
mod relogin_registry_commit_tests;
#[cfg(test)]
#[path = "setup/relogin_temp_home.test.rs"]
mod relogin_temp_home_tests;
#[cfg(test)]
#[path = "setup.test.rs"]
mod tests;
