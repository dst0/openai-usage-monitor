use crate::models::{AccountConfig, AccountsFile, AuthJson};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

#[path = "storage/accounts_registry_transaction_service.rs"]
mod accounts_registry_transaction_service;
#[path = "storage/active_auth_compare_write_service.rs"]
mod active_auth_compare_write_service;
#[path = "storage/active_auth_create_service.rs"]
mod active_auth_create_service;
#[path = "storage/active_auth_remove_service.rs"]
mod active_auth_remove_service;
#[path = "storage/codex_home_resolver.rs"]
mod codex_home_resolver;
pub(crate) use active_auth_remove_service::compare_and_remove_active_auth_json;
#[path = "storage/status_file_service.rs"]
mod status_file_service;
pub use status_file_service::{sync_settings_to_status_file, write_status_file};

#[cfg(test)]
#[path = "storage/test_codex_home.rs"]
pub(crate) mod test_codex_home;

pub use codex_home_resolver::codex_home;

pub fn auth_json_path() -> PathBuf {
    codex_home().join("auth.json")
}

pub fn accounts_json_path() -> PathBuf {
    codex_home().join("accounts.json")
}

pub fn status_json_path() -> PathBuf {
    codex_home().join("usage-status.json")
}

pub fn daemon_lock_path() -> PathBuf {
    codex_home().join("daemon.lock")
}

pub fn switcher_lock_path() -> PathBuf {
    codex_home().join("codex.lock")
}

fn acquire_switcher_lock(exclusive: bool) -> Result<File, String> {
    let lock_path = switcher_lock_path();
    if let Some(parent) = lock_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("Failed to open lockfile {}: {}", lock_path.display(), e))?;
    let _ = fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600));

    if exclusive {
        file.lock_exclusive()
            .map_err(|e| format!("Failed to acquire exclusive switcher lock: {}", e))?;
    } else {
        file.lock_shared()
            .map_err(|e| format!("Failed to acquire shared switcher lock: {}", e))?;
    }
    Ok(file)
}

pub fn read_active_auth_json() -> Result<AuthJson, String> {
    read_active_auth_json_with_hook(|| {})
}

fn read_active_auth_json_with_hook(after_open: impl FnOnce()) -> Result<AuthJson, String> {
    let path = auth_json_path();
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|_| "Active credential file could not be opened safely".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "Active credential identity could not be read".to_string())?;
    if !opened.is_file() || opened.permissions().mode() & 0o777 != 0o600 {
        return Err("Active credential file is not a private regular file".into());
    }
    file.lock_shared()
        .map_err(|_| "Active credential file could not be locked".to_string())?;
    after_open();

    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|_| "Active credential file could not be read".to_string())?;

    let named = fs::symlink_metadata(&path)
        .map_err(|_| "Active credential identity changed during read".to_string())?;
    if !named.is_file() || named.dev() != opened.dev() || named.ino() != opened.ino() {
        return Err("Active credential identity changed during read".into());
    }

    let auth: AuthJson = serde_json::from_str(&content)
        .map_err(|_| "Active credential file could not be parsed".to_string())?;
    Ok(auth)
}

pub fn write_active_auth_json(auth: &AuthJson) -> Result<(), String> {
    // Serialize Monitor writers with registry updates. The official Desktop
    // does not participate in this advisory lock; relogin uses an additional
    // pathname/inode/content comparison immediately before its rename.
    let _lock = acquire_switcher_lock(true)?;
    let path = auth_json_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut nonce = [0u8; 8];
    getrandom::getrandom(&mut nonce)
        .map_err(|_| "Active credential staging nonce unavailable".to_string())?;
    let temp_path = path.with_extension(format!(
        "{}.{:016x}.tmp.json",
        std::process::id(),
        u64::from_ne_bytes(nonce)
    ));
    let content = serde_json::to_string_pretty(auth)
        .map_err(|e| format!("Failed to serialize auth.json: {}", e))?;
    let mut created = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(&temp_path)
            .map_err(|_| "Temporary credential file could not be created".to_string())?;
        created = true;

        file.lock_exclusive()
            .map_err(|_| "Temporary credential file could not be locked".to_string())?;
        file.write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Temporary credential file could not be saved".to_string())?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| "Temporary credential permissions could not be set".to_string())?;
        fs::rename(&temp_path, &path)
            .map_err(|_| "Active credential replacement failed".to_string())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

pub(crate) fn write_active_auth_json_if_absent(
    auth: &AuthJson,
    shared_auth_active: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    active_auth_create_service::ActiveAuthCreateService::create_if_absent(auth, shared_auth_active)
}

pub(crate) fn compare_and_write_active_auth_json(
    expected: &AuthJson,
    replacement: &AuthJson,
    shared_auth_active: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let path = auth_json_path();
    let _lock = acquire_switcher_lock(true)?;
    active_auth_compare_write_service::ActiveAuthCompareWriteService::new(&path).execute(
        expected,
        replacement,
        shared_auth_active,
    )
}

pub(crate) fn compare_and_write_active_auth_json_for_switch(
    expected: &AuthJson,
    replacement: &AuthJson,
    shared_auth_active: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let path = auth_json_path();
    let _lock = acquire_switcher_lock(true)?;
    active_auth_compare_write_service::ActiveAuthCompareWriteService::new(&path)
        .execute_replacing_tokens(expected, replacement, shared_auth_active)
}

pub fn load_accounts() -> Result<AccountsFile, String> {
    load_accounts_with_hooks(|| {}, || {})
}

fn load_accounts_with_hooks(
    before_initialize: impl FnOnce(),
    before_heal: impl FnOnce(),
) -> Result<AccountsFile, String> {
    let path = accounts_json_path();
    if !path.exists() {
        // Try auto-importing from current auth.json
        if let Ok(auth) = read_active_auth_json() {
            if let Some(tokens) = auth.tokens {
                let (email, plan) = crate::oauth::extract_jwt_metadata_from_tokens(&tokens);
                let acc_id = tokens
                    .account_id
                    .clone()
                    .unwrap_or_else(|| "default".to_string());
                let email_val = email.unwrap_or_else(|| "current-user".to_string());
                let canonical_id = crate::setup::build_predictable_account_id(&email_val, &acc_id);
                let config = AccountConfig {
                    id: canonical_id.clone(),
                    name: Some("main".to_string()),
                    email: email_val,
                    plan_type: plan.unwrap_or_else(|| "team".to_string()),
                    account_id: acc_id,
                    tokens,
                    enabled: true,
                    priority: 1,
                    last_primary_percentage: 100.0,
                    last_reset_time: None,
                    last_reset_after_seconds: None,
                    last_weekly_percentage: None,
                    last_weekly_reset_time: None,
                    last_weekly_reset_after_seconds: None,
                    last_credits: None,
                    last_error: None,
                    last_checked: None,
                    plan_multiplier: None,
                    multiplier_is_manual: None,
                    last_multiplier_checked: None,
                    organization_name: None,
                };
                let accounts_file = AccountsFile {
                    active_account_id: Some(canonical_id),
                    settings: crate::models::Settings::default(),
                    accounts: vec![config],
                };
                before_initialize();
                return accounts_registry_transaction_service::AccountsRegistryTransactionService::initialize_if_absent(&accounts_file);
            }
        }
        return Ok(AccountsFile::default());
    }

    let mut acc: AccountsFile = {
        let _lock = acquire_switcher_lock(false)?;

        let mut file =
            File::open(&path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        file.lock_shared()
            .map_err(|e| format!("Failed to lock shared {}: {}", path.display(), e))?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?
    };

    // Auto-heal duplicate identities from a fresh snapshot under one registry
    // lock. The first read is only a signal; another writer may have committed
    // newer tokens before this process gets the exclusive lock.
    if crate::setup::deduplicate_accounts_file(&mut acc) {
        before_heal();
        return update_accounts_atomically(|fresh| {
            crate::setup::deduplicate_accounts_file(fresh);
            Ok(())
        });
    }

    Ok(acc)
}

#[cfg(test)]
pub fn save_accounts(acc: &AccountsFile) -> Result<(), String> {
    accounts_registry_transaction_service::AccountsRegistryTransactionService::save(acc)
}

pub(crate) fn initialize_accounts_if_absent(
    initial: &AccountsFile,
) -> Result<AccountsFile, String> {
    accounts_registry_transaction_service::AccountsRegistryTransactionService::initialize_if_absent(
        initial,
    )
}

pub fn update_accounts_atomically(
    update: impl FnOnce(&mut AccountsFile) -> Result<(), String>,
) -> Result<AccountsFile, String> {
    accounts_registry_transaction_service::AccountsRegistryTransactionService::update(update)
}

#[cfg(test)]
#[path = "storage/active_auth_read.test.rs"]
mod active_auth_read_tests;
