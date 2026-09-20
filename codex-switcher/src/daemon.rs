use crate::distribution::daemon_account_sync_service::DaemonAccountSyncService;
use crate::distribution::daemon_loop_service::DaemonLoopService;
use crate::distribution::daemon_tick_service::DaemonTickService;
use crate::models::AccountsFile;

pub fn sync_active_tokens(accounts_file: &mut AccountsFile) -> Result<bool, String> {
    DaemonAccountSyncService::sync_active_tokens(accounts_file)
}

#[allow(dead_code)]
pub fn run_daemon_tick() -> Result<(), String> {
    DaemonTickService::run(true)
}

pub fn refresh_quotas_and_status() -> Result<(), String> {
    DaemonTickService::run(false)
}

pub fn run_daemon_loop() {
    DaemonLoopService::run()
}
