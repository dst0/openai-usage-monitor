use crate::models::{AccountConfig, WhamUsageResponse};
use crate::oauth::refresh_access_token;
use chrono::{DateTime, Utc};

pub const WHAM_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const ACCOUNTS_CHECK_URL: &str = "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27";
#[path = "quota/reset_credit_consumption.rs"]
mod reset_credit_consumption;

#[cfg(test)]
use reset_credit_consumption::consume_rate_limit_reset_credit_at;
pub(crate) use reset_credit_consumption::{
    consume_rate_limit_reset_credit, ResetCreditConsumeOutcome,
};

pub fn detect_account_multiplier(account: &mut AccountConfig) -> f64 {
    let now_iso = Utc::now().to_rfc3339();
    account.last_multiplier_checked = Some(now_iso);

    if account.multiplier_is_manual == Some(true) {
        if let Some(m) = account.plan_multiplier {
            return m;
        }
    }

    let mut detected = match account.plan_type.to_lowercase().as_str() {
        "pro" => 20.0,
        "team" | "business" => 1.0,
        "plus" => 1.0,
        "free" => 0.2,
        _ => 1.0,
    };

    let mut req = ureq::get(ACCOUNTS_CHECK_URL)
        .set(
            "Authorization",
            &format!("Bearer {}", account.tokens.access_token),
        )
        .set("User-Agent", "Codex/1.0")
        .set("Accept", "application/json");

    if let Some(acc_id) = &account.tokens.account_id {
        req = req.set("ChatGPT-Account-Id", acc_id);
    } else if !account.account_id.is_empty() {
        req = req.set("ChatGPT-Account-Id", &account.account_id);
    }

    if let Ok(resp) = req.call() {
        if let Ok(json_val) = resp.into_json::<serde_json::Value>() {
            if let Some(accounts_map) = json_val.get("accounts").and_then(|a| a.as_object()) {
                let target_acc = accounts_map
                    .get(&account.account_id)
                    .or_else(|| {
                        accounts_map.values().find(|item| {
                            if let Some(sub_id) = item
                                .get("account")
                                .and_then(|a| a.get("account_id").or_else(|| a.get("id")))
                                .and_then(|id| id.as_str())
                            {
                                if !account.account_id.is_empty() && sub_id == account.account_id {
                                    return true;
                                }
                            }
                            false
                        })
                    })
                    .or_else(|| accounts_map.get("default"))
                    .or_else(|| accounts_map.values().next());

                if let Some(item) = target_acc {
                    if let Some(org_name) = item
                        .get("account")
                        .and_then(|a| a.get("name"))
                        .and_then(|n| n.as_str())
                    {
                        let trimmed = org_name.trim();
                        if !trimmed.is_empty() {
                            account.organization_name = Some(trimmed.to_string());
                        }
                    }

                    let sub_plan = item
                        .get("entitlement")
                        .and_then(|e| e.get("subscription_plan"))
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_lowercase();

                    let plan_type = item
                        .get("account")
                        .and_then(|a| a.get("plan_type"))
                        .and_then(|p| p.as_str())
                        .unwrap_or(&account.plan_type)
                        .to_lowercase();

                    let features = item
                        .get("features")
                        .and_then(|f| f.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                        .unwrap_or_default();

                    if plan_type == "pro" {
                        if sub_plan.contains("100")
                            || sub_plan.contains("lite")
                            || sub_plan.contains("5x")
                        {
                            detected = 5.0;
                        } else {
                            detected = 20.0;
                        }
                    } else if plan_type == "team" || plan_type == "business" {
                        if features.iter().any(|&f| {
                            f == "self_serve_business_prolite" || f.contains("premium_seat")
                        }) {
                            detected = 5.0;
                        } else {
                            detected = 1.0;
                        }
                    } else if plan_type == "plus" {
                        detected = 1.0;
                    } else if plan_type == "free" {
                        detected = 0.2;
                    }
                }
            }
        }
    }

    account.plan_multiplier = Some(detected);
    detected
}

pub fn fetch_account_usage(account: &mut AccountConfig) -> Result<WhamUsageResponse, String> {
    let mut tried_refresh = false;

    loop {
        let mut req = ureq::get(WHAM_USAGE_URL)
            .set(
                "Authorization",
                &format!("Bearer {}", account.tokens.access_token),
            )
            .set("User-Agent", "Codex/1.0")
            .set("Accept", "application/json");

        if let Some(acc_id) = &account.tokens.account_id {
            req = req.set("ChatGPT-Account-Id", acc_id);
        } else if !account.account_id.is_empty() {
            req = req.set("ChatGPT-Account-Id", &account.account_id);
        }

        match req.call() {
            Ok(resp) => {
                let usage: WhamUsageResponse = resp
                    .into_json()
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

    let should_detect = (account.multiplier_is_manual != Some(true)
        && (account.plan_multiplier.is_none()
            || account.last_multiplier_checked.as_deref().is_none_or(|ts| {
                chrono::DateTime::parse_from_rfc3339(ts).map_or(true, |dt| {
                    (Utc::now() - dt.with_timezone(&Utc)).num_seconds() > 7 * 86400
                })
            })))
        || (account.is_business() && account.organization_name.is_none());

    if should_detect {
        detect_account_multiplier(account);
    }

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
                    if account.multiplier_is_manual != Some(true)
                        && (account.plan_type == "team" || account.plan_type == "business")
                    {
                        if prim.limit_window_seconds > 86400 {
                            account.plan_multiplier = Some(5.0);
                        } else {
                            account.plan_multiplier = Some(1.0);
                        }
                    }

                    let mult = account.effective_multiplier();
                    let raw_pct = (100.0 - prim.used_percent).clamp(0.0, 100.0);
                    account.last_primary_percentage = raw_pct * mult;
                    account.last_reset_after_seconds = Some(prim.reset_after_seconds);
                    if prim.reset_at > 0 {
                        if let Some(dt) = DateTime::from_timestamp(prim.reset_at, 0) {
                            account.last_reset_time = Some(dt.to_rfc3339());
                        }
                    }
                    if prim.limit_window_seconds > 86400 && rl.secondary_window.is_none() {
                        account.last_weekly_percentage = Some(raw_pct * mult);
                        account.last_weekly_reset_after_seconds = Some(prim.reset_after_seconds);
                        account.last_weekly_reset_time = account.last_reset_time.clone();
                    }
                }
                if let Some(sec) = rl.secondary_window {
                    let mult = account.effective_multiplier();
                    let raw_sec_pct = (100.0 - sec.used_percent).clamp(0.0, 100.0);
                    account.last_weekly_percentage = Some(raw_sec_pct * mult);
                    account.last_weekly_reset_after_seconds = Some(sec.reset_after_seconds);
                    if sec.reset_at > 0 {
                        if let Some(dt) = DateTime::from_timestamp(sec.reset_at, 0) {
                            account.last_weekly_reset_time = Some(dt.to_rfc3339());
                        }
                    }
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

#[cfg(test)]
#[path = "quota.test.rs"]
mod tests;
