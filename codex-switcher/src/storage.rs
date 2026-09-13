use crate::models::{AccountConfig, AccountsFile, AuthJson, StatusFile};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn codex_home() -> PathBuf {
    if let Ok(p) = std::env::var("CODEX_HOME") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    dirs::home_dir()
        .map(|h| h.join(".codex"))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

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
    let path = auth_json_path();
    if !path.exists() {
        return Err(format!("Auth file not found at {}", path.display()));
    }
    let mut file =
        File::open(&path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    file.lock_shared()
        .map_err(|e| format!("Failed to lock shared {}: {}", path.display(), e))?;

    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let auth: AuthJson = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;
    Ok(auth)
}

pub fn write_active_auth_json(auth: &AuthJson) -> Result<(), String> {
    let path = auth_json_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let temp_path = path.with_extension(format!("{}.tmp.json", std::process::id()));
    let content = serde_json::to_string_pretty(auth)
        .map_err(|e| format!("Failed to serialize auth.json: {}", e))?;

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&temp_path)
            .map_err(|e| {
                format!(
                    "Failed to open temp auth file {}: {}",
                    temp_path.display(),
                    e
                )
            })?;

        file.lock_exclusive()
            .map_err(|e| format!("Failed to lock temp auth file: {}", e))?;

        file.set_len(0)
            .map_err(|e| format!("Failed to set_len: {}", e))?;
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek: {}", e))?;

        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write to temp auth file: {}", e))?;
        file.flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;

        let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
    }

    fs::rename(&temp_path, &path)
        .map_err(|e| format!("Failed to replace {}: {}", path.display(), e))?;
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    Ok(())
}

pub fn load_accounts() -> Result<AccountsFile, String> {
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
                let _ = save_accounts(&accounts_file);
                return Ok(accounts_file);
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

    // Auto-heal deduplication if duplicates exist in accounts.json
    // Lock from above was dropped at block exit, so save_accounts can safely acquire exclusive lock!
    if crate::setup::deduplicate_accounts_file(&mut acc) {
        let _ = save_accounts(&acc);
    }

    Ok(acc)
}

pub fn save_accounts(acc: &AccountsFile) -> Result<(), String> {
    let _lock = acquire_switcher_lock(true)?;

    let path = accounts_json_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let temp_path = path.with_extension(format!("{}.tmp.json", std::process::id()));
    let content = serde_json::to_string_pretty(acc)
        .map_err(|e| format!("Failed to serialize accounts.json: {}", e))?;

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&temp_path)
            .map_err(|e| format!("Failed to open temp accounts file: {}", e))?;

        file.lock_exclusive()
            .map_err(|e| format!("Failed to lock temp accounts file: {}", e))?;

        file.set_len(0)
            .map_err(|e| format!("Failed to set_len: {}", e))?;
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek: {}", e))?;

        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write: {}", e))?;
        file.flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;

        let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
    }

    fs::rename(&temp_path, &path)
        .map_err(|e| format!("Failed to replace {}: {}", path.display(), e))?;
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    Ok(())
}

pub fn write_status_file(status: &StatusFile) -> Result<(), String> {
    let path = status_json_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    let content = serde_json::to_string_pretty(status)
        .map_err(|e| format!("Failed to serialize usage-status.json: {}", e))?;

    let temp_path = path.with_extension(format!("{}.tmp.json", std::process::id()));
    fs::write(&temp_path, content.as_bytes())
        .map_err(|e| format!("Failed to write temp status file: {}", e))?;
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&temp_path, &path).map_err(|e| format!("Failed to rename status file: {}", e))?;
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn read_status_file() -> Result<StatusFile, String> {
    let path = status_json_path();
    if !path.exists() {
        return Err(format!("Status file not found at {}", path.display()));
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read status file: {}", e))?;
    let status: StatusFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse status file: {}", e))?;
    Ok(status)
}

pub fn sync_settings_to_status_file(settings: &crate::models::Settings) {
    if let Ok(mut status) = read_status_file() {
        status.auto_switch_enabled = settings.auto_switch_enabled;
        status.auto_switch_business_only = settings.auto_switch_business_only;
        status.auto_switch_business_priority = settings.auto_switch_business_priority;
        status.auto_reset_weekly_enabled = settings.auto_reset_weekly_enabled;
        status.auto_reset_weekly_min_remaining_seconds =
            settings.auto_reset_weekly_min_remaining_seconds;
        if !settings.auto_reset_weekly_enabled {
            status.auto_reset_state = "disabled".to_string();
            status.auto_reset_reason = None;
        }
        let _ = write_status_file(&status);
    }
}
