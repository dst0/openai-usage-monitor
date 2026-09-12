use crate::models::{AccountConfig, WhamUsageResponse};
use crate::oauth::refresh_access_token;
use chrono::{DateTime, Utc};

pub const WHAM_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

pub fn fetch_account_usage(account: &mut AccountConfig) -> Result<WhamUsageResponse, String> {
    let mut tried_refresh = false;

    loop {
        let mut req = ureq::get(WHAM_USAGE_URL)
            .set("Authorization", &format!("Bearer {}", account.tokens.access_token))
            .set("User-Agent", "Codex/1.0")
            .set("Accept", "application/json");

        if let Some(acc_id) = &account.tokens.account_id {
            req = req.set("ChatGPT-Account-Id", acc_id);
        } else if !account.account_id.is_empty() {
            req = req.set("ChatGPT-Account-Id", &account.account_id);
        }

        match req.call() {
            Ok(resp) => {
                let usage: WhamUsageResponse = resp.into_json()
                    .map_err(|e| format!("Failed to parse usage JSON: {}", e))?;
                return Ok(usage);
            }
            Err(ureq::Error::Status(401, _)) if !tried_refresh => {
                tried_refresh = true;
                match refresh_access_token(&mut account.tokens) {
                    Ok(()) => continue,
                    Err(e) => return Err(format!("401 Unauthorized ({})", e)),
                }
            }
            Err(e) => {
                return Err(format!("Failed to fetch usage: {}", e));
            }
        }
    }
}

pub fn update_account_quota_cache(account: &mut AccountConfig) {
    let now_iso = Utc::now().to_rfc3339();
    account.last_checked = Some(now_iso);

    match fetch_account_usage(account) {
        Ok(usage) => {
            if let Some(email) = usage.email {
                account.email = email;
            }
            if let Some(plan) = usage.plan_type {
                account.plan_type = plan;
            }
            if let Some(rl) = usage.rate_limit {
                if let Some(prim) = rl.primary_window {
                    account.last_primary_percentage = (100.0 - prim.used_percent).clamp(0.0, 100.0);
                    account.last_reset_after_seconds = Some(prim.reset_after_seconds);
                    if prim.reset_at > 0 {
                        if let Some(dt) = DateTime::from_timestamp(prim.reset_at, 0) {
                            account.last_reset_time = Some(dt.to_rfc3339());
                        }
                    }
                    if prim.limit_window_seconds > 86400 && rl.secondary_window.is_none() {
                        account.last_weekly_percentage = Some((100.0 - prim.used_percent).clamp(0.0, 100.0));
                    }
                }
                if let Some(sec) = rl.secondary_window {
                    account.last_weekly_percentage = Some((100.0 - sec.used_percent).clamp(0.0, 100.0));
                }
            }
            if let Some(credits) = usage.rate_limit_reset_credits {
                account.last_credits = Some(credits.available_count);
            }
            account.last_error = None;
        }
        Err(err) => {
            account.last_error = Some(err);
        }
    }
}

pub fn format_reset_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "now".to_string();
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}
