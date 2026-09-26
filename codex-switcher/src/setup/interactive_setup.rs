use crate::models::{AccountsFile, AuthJson, AuthTokens};
use crate::storage::{
    auth_json_path, codex_home, load_accounts, read_active_auth_json, write_active_auth_json,
};
use std::io::{self, BufRead, Write};
use std::process::Command;

use super::{add_account_from_tokens, relogin_temp_home::ReloginTempHome, save_current_as};

pub fn run_interactive_setup() -> Result<(), String> {
    println!("==================================================");
    println!("🤖 OpenAI Codex Multi-Account Setup");
    println!("==================================================");

    let accounts_file = load_login_accounts_with(load_accounts)?;
    println!(
        "Current configured accounts: {}",
        accounts_file.accounts.len()
    );
    for acc in &accounts_file.accounts {
        let active = if accounts_file.active_account_id.as_deref() == Some(&acc.id) {
            " (active)"
        } else {
            ""
        };
        println!(
            "  - {} <{}> [{}]{}",
            acc.display_name(),
            acc.email,
            acc.plan_type,
            active
        );
    }
    println!("--------------------------------------------------");
    println!("1. Save current session as a new/updated account");
    println!("2. Add second/next account via Codex CLI login");
    println!("3. Test API quotas for all accounts now");
    println!("4. Exit");
    print!("Select [1-4]: ");
    io::stdout().flush().unwrap();

    let mut line = String::new();
    let stdin = io::stdin();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;

    match line.trim() {
        "1" => {
            print!("Enter a nickname for this account (e.g. personal, work): ");
            io::stdout().flush().unwrap();
            let mut id = String::new();
            stdin.lock().read_line(&mut id).map_err(|e| e.to_string())?;
            let id = id.trim();
            save_current_as(id)?;
        }
        "2" => {
            print!("Enter a nickname for the new account (e.g. personal, work): ");
            io::stdout().flush().unwrap();
            let mut id = String::new();
            stdin.lock().read_line(&mut id).map_err(|e| e.to_string())?;
            let id = id.trim();
            login_and_add_account(id)?;
        }
        "3" => {
            crate::daemon::refresh_quotas_and_status()?;
        }
        _ => {
            println!("Exited.");
        }
    }

    Ok(())
}

pub fn resolve_codex_bin() -> Result<std::path::PathBuf, String> {
    crate::codex_binary_path::resolve_real_codex_bin()
}

pub fn login_and_add_account(id: &str) -> Result<(), String> {
    let codex_bin = resolve_codex_bin()?;
    login_and_add_account_with_codex_bin(id, &codex_bin)
}

fn login_and_add_account_with_codex_bin(
    id: &str,
    codex_bin: impl AsRef<std::ffi::OsStr>,
) -> Result<(), String> {
    let id_trimmed = id.trim();
    if id_trimmed.is_empty() {
        println!("🌐 Launching Codex login in browser...");
    } else {
        println!(
            "🌐 Launching Codex login in browser for account '{}'...",
            id_trimmed
        );
    }

    // Browser login uses the same random, private CODEX_HOME lifecycle as
    // re-login; it never removes a pathname belonging to another process.
    let temp_home = ReloginTempHome::new()?;
    let temp_dir = temp_home.path();

    // If config.toml exists in real CODEX_HOME, copy it to temp_dir so custom network/proxy settings are honored
    let real_home = codex_home();
    let real_config = real_home.join("config.toml");
    if real_config.exists() {
        let _ = std::fs::copy(&real_config, temp_dir.join("config.toml"));
    }

    let status = Command::new(codex_bin)
        .arg("login")
        .env("CODEX_HOME", &temp_dir)
        .status()
        .map_err(|e| format!("Failed to run codex login: {}", e))?;

    if !status.success() {
        return Err("Codex login failed or was cancelled.".to_string());
    }

    let temp_auth_path = temp_dir.join("auth.json");
    if !temp_auth_path.exists() {
        return Err("Login succeeded but no credentials were generated.".to_string());
    }

    let auth_content = std::fs::read_to_string(&temp_auth_path)
        .map_err(|e| format!("Failed to read new auth file: {}", e))?;
    let new_auth: AuthJson = serde_json::from_str(&auth_content)
        .map_err(|e| format!("Failed to parse new auth file: {}", e))?;
    let tokens = new_auth
        .tokens
        .ok_or_else(|| "No tokens found in new auth session".to_string())?;

    let mut accounts_file = load_login_accounts_with(load_accounts)?;

    // If accounts_file has no accounts configured, but ~/.codex/auth.json has active credentials,
    // auto-save the active session first so the original account isn't lost!
    preserve_existing_active_session_with(
        &mut accounts_file,
        read_active_auth_if_present,
        |accounts, tokens| add_account_from_tokens(accounts, "", tokens, false),
    )?;

    let had_active_account = accounts_file
        .active_account_id
        .as_ref()
        .map(|act| accounts_file.accounts.iter().any(|a| a.id == *act))
        .unwrap_or(false);

    let target_id = add_account_from_tokens(&mut accounts_file, id_trimmed, tokens.clone(), true)?;

    // If there was no active account previously, initialize ~/.codex/auth.json with the new tokens
    if !had_active_account {
        if crate::switcher::is_shared_auth_active_checked()? {
            return Err("Shared credentials became active before first-account setup".into());
        }
        let new_live_auth = match read_active_auth_if_present()? {
            Some(mut current_auth) if current_auth.tokens.is_none() => {
                current_auth.tokens = Some(tokens);
                current_auth
            }
            Some(_) => return Err("Active credentials changed during first-account setup".into()),
            None => AuthJson {
                auth_mode: Some("chatgpt".to_string()),
                openai_api_key: None,
                tokens: Some(tokens),
                last_refresh: Some(chrono::Utc::now().to_rfc3339()),
                extra: Default::default(),
            },
        };
        write_active_auth_json(&new_live_auth)?;
    }

    // Refresh quotas and status file without auto-switching or app restarts
    let _ = crate::daemon::refresh_quotas_and_status();

    let saved_email = accounts_file
        .accounts
        .iter()
        .find(|a| a.id == target_id)
        .map(|a| a.email.as_str())
        .unwrap_or("");
    println!(
        "🎉 Successfully logged in and added account '{}' ({})! Active account preserved.",
        target_id, saved_email
    );

    Ok(())
}

fn load_login_accounts_with(
    load: impl FnOnce() -> Result<AccountsFile, String>,
) -> Result<AccountsFile, String> {
    load()
}

fn preserve_existing_active_session_with(
    accounts: &mut AccountsFile,
    read_auth: impl FnOnce() -> Result<Option<AuthJson>, String>,
    save: impl FnOnce(&mut AccountsFile, AuthTokens) -> Result<String, String>,
) -> Result<(), String> {
    if accounts.accounts.is_empty() {
        if let Some(auth) = read_auth()? {
            if let Some(tokens) = auth.tokens {
                save(accounts, tokens)?;
            }
        }
    }
    Ok(())
}

fn read_active_auth_if_present() -> Result<Option<AuthJson>, String> {
    match std::fs::symlink_metadata(auth_json_path()) {
        Ok(metadata) if metadata.file_type().is_file() => read_active_auth_json().map(Some),
        Ok(_) => Err("Active credential path is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Active credential presence could not be checked".into()),
    }
}

#[cfg(test)]
#[path = "interactive_setup.test.rs"]
mod tests;
