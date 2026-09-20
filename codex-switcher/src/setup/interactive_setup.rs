use crate::models::AuthJson;
use crate::storage::{codex_home, load_accounts, read_active_auth_json, write_active_auth_json};
use std::io::{self, BufRead, Write};
use std::process::Command;

use super::{add_account_from_tokens, save_current_as};

pub fn run_interactive_setup() -> Result<(), String> {
    println!("==================================================");
    println!("🤖 OpenAI Codex Multi-Account Setup");
    println!("==================================================");

    let accounts_file = load_accounts().unwrap_or_default();
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

pub fn resolve_codex_bin() -> String {
    let app_codex = "/Applications/ChatGPT.app/Contents/Resources/codex";
    if std::path::Path::new(app_codex).exists() {
        return app_codex.to_string();
    }
    "codex".to_string()
}

pub fn login_and_add_account(id: &str) -> Result<(), String> {
    let id_trimmed = id.trim();
    if id_trimmed.is_empty() {
        println!("🌐 Launching Codex login in browser...");
    } else {
        println!(
            "🌐 Launching Codex login in browser for account '{}'...",
            id_trimmed
        );
    }

    // 1. Create a secure, isolated temporary directory for CODEX_HOME
    // so that the active ~/.codex/auth.json and ChatGPT.app session remain completely untouched.
    let temp_dir_name = format!("codex-login-{}", std::process::id());
    let temp_dir = std::env::temp_dir().join(temp_dir_name);
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temporary login directory: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp_dir, std::fs::Permissions::from_mode(0o700));
    }

    struct TempDirGuard<'a>(&'a std::path::Path);
    impl<'a> Drop for TempDirGuard<'a> {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }
    let _guard = TempDirGuard(&temp_dir);

    // If config.toml exists in real CODEX_HOME, copy it to temp_dir so custom network/proxy settings are honored
    let real_home = codex_home();
    let real_config = real_home.join("config.toml");
    if real_config.exists() {
        let _ = std::fs::copy(&real_config, temp_dir.join("config.toml"));
    }

    let codex_bin = resolve_codex_bin();
    let status = Command::new(&codex_bin)
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

    let mut accounts_file = load_accounts().unwrap_or_default();

    // If accounts_file has no accounts configured, but ~/.codex/auth.json has active credentials,
    // auto-save the active session first so the original account isn't lost!
    if accounts_file.accounts.is_empty() {
        if let Ok(active_auth) = read_active_auth_json() {
            if let Some(active_tokens) = active_auth.tokens {
                let _ = add_account_from_tokens(&mut accounts_file, "", active_tokens, false);
            }
        }
    }

    let had_active_account = accounts_file
        .active_account_id
        .as_ref()
        .map(|act| accounts_file.accounts.iter().any(|a| a.id == *act))
        .unwrap_or(false);

    let target_id = add_account_from_tokens(&mut accounts_file, id_trimmed, tokens.clone(), true)?;

    // If there was no active account previously, initialize ~/.codex/auth.json with the new tokens
    if !had_active_account {
        if let Ok(mut current_auth) = read_active_auth_json() {
            if current_auth.tokens.is_none() {
                current_auth.tokens = Some(tokens);
                let _ = write_active_auth_json(&current_auth);
            }
        } else {
            let new_live_auth = AuthJson {
                auth_mode: Some("chatgpt".to_string()),
                openai_api_key: None,
                tokens: Some(tokens),
                last_refresh: Some(chrono::Utc::now().to_rfc3339()),
            };
            let _ = write_active_auth_json(&new_live_auth);
        }
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
