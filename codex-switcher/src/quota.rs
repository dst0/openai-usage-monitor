use crate::models::{AccountConfig, WhamUsageResponse};
use crate::oauth::refresh_access_token;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

pub const WHAM_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const ACCOUNTS_CHECK_URL: &str = "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27";
const RATE_LIMIT_RESET_CONSUME_URL: &str =
    "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";
const MAX_RESET_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResetCreditConsumeOutcome {
    Applied,
    NotConsumed(String),
    /// The service definitely rejected the request before redeeming a credit.
    /// A later daemon tick may retry the same logical operation.
    Unavailable(String),
    /// The request may have reached the service. The persisted idempotency key
    /// must be reused and account switching must wait for a terminal outcome.
    Unknown(String),
}

#[derive(Debug, Deserialize)]
struct ResetCreditConsumeResponse {
    code: String,
    #[serde(default)]
    windows_reset: i64,
}

fn reset_account_route(account: &AccountConfig) -> Result<&str, String> {
    let configured = account.account_id.trim();
    let token_account = account.tokens.account_id.as_deref().map(str::trim);
    if token_account.is_some_and(|value| !configured.is_empty() && value != configured) {
        return Err("active_account_route_mismatch".into());
    }
    let route = token_account
        .filter(|value| !value.is_empty())
        .or_else(|| (!configured.is_empty()).then_some(configured))
        .ok_or_else(|| "active_account_route_missing".to_string())?;
    if route.len() > 512
        || !route
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\')
    {
        return Err("active_account_route_invalid".into());
    }
    Ok(route)
}

fn parse_reset_credit_response(
    response: ureq::Response,
) -> Result<ResetCreditConsumeOutcome, String> {
    let mut bytes = Vec::with_capacity(1024);
    response
        .into_reader()
        .take(MAX_RESET_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "reset_service_response_read_failed".to_string())?;
    if bytes.len() as u64 > MAX_RESET_RESPONSE_BYTES {
        return Err("reset_service_response_too_large".into());
    }
    let payload: ResetCreditConsumeResponse =
        serde_json::from_slice(&bytes).map_err(|_| "reset_service_response_invalid".to_string())?;
    let _windows_reset = payload.windows_reset;
    match payload.code.as_str() {
        "reset" | "already_redeemed" | "alreadyRedeemed" => Ok(ResetCreditConsumeOutcome::Applied),
        "nothing_to_reset" | "nothingToReset" | "no_credit" | "noCredit" => {
            Ok(ResetCreditConsumeOutcome::NotConsumed(payload.code))
        }
        _ => Err("reset_service_outcome_unknown".into()),
    }
}

fn consume_rate_limit_reset_credit_at(
    endpoint: &str,
    account: &AccountConfig,
    idempotency_key: &str,
) -> ResetCreditConsumeOutcome {
    if idempotency_key.is_empty()
        || idempotency_key.len() > 512
        || !idempotency_key.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return ResetCreditConsumeOutcome::Unavailable("invalid_idempotency_key".into());
    }
    let route = match reset_account_route(account) {
        Ok(route) => route,
        Err(reason) => return ResetCreditConsumeOutcome::Unavailable(reason),
    };
    let token = account.tokens.access_token.trim();
    if token.is_empty() || token.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return ResetCreditConsumeOutcome::Unavailable("active_access_token_invalid".into());
    }
    let request = ureq::post(endpoint)
        .timeout(Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {token}"))
        .set("ChatGPT-Account-Id", route)
        .set("User-Agent", "Codex/1.0")
        .set("Accept", "application/json")
        .set("Content-Type", "application/json");
    let response = request.send_json(serde_json::json!({
        "redeem_request_id": idempotency_key
    }));
    match response {
        Ok(response) => {
            parse_reset_credit_response(response).unwrap_or_else(ResetCreditConsumeOutcome::Unknown)
        }
        Err(ureq::Error::Status(status, response)) => {
            if let Ok(outcome) = parse_reset_credit_response(response) {
                return outcome;
            }
            if status >= 500 {
                ResetCreditConsumeOutcome::Unknown(format!("reset_service_http_{status}"))
            } else {
                ResetCreditConsumeOutcome::Unavailable(format!("reset_service_http_{status}"))
            }
        }
        Err(ureq::Error::Transport(_)) => {
            ResetCreditConsumeOutcome::Unknown("reset_service_transport_uncertain".into())
        }
    }
}

/// Redeems one earned reset through the same authenticated ChatGPT backend
/// used by the quota reader. This is account-scoped and does not load, create,
/// or write a Codex thread, so Desktop remains the sole thread runtime owner.
pub(crate) fn consume_rate_limit_reset_credit(
    account: &AccountConfig,
    idempotency_key: &str,
) -> ResetCreditConsumeOutcome {
    consume_rate_limit_reset_credit_at(RATE_LIMIT_RESET_CONSUME_URL, account, idempotency_key)
}

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
mod tests {
    use super::*;
    use crate::models::AuthTokens;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    fn test_account() -> AccountConfig {
        AccountConfig {
            id: "active".into(),
            name: None,
            email: "person@example.invalid".into(),
            plan_type: "pro".into(),
            account_id: "workspace-123".into(),
            tokens: AuthTokens {
                access_token: "secret-test-token".into(),
                refresh_token: None,
                id_token: None,
                account_id: Some("workspace-123".into()),
            },
            enabled: true,
            priority: 0,
            last_primary_percentage: 0.0,
            last_reset_time: None,
            last_reset_after_seconds: None,
            last_weekly_percentage: Some(0.0),
            last_weekly_reset_time: None,
            last_weekly_reset_after_seconds: Some(86_400),
            last_credits: Some(1),
            last_error: None,
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
            organization_name: None,
        }
    }

    fn mock_reset_endpoint(response_body: &'static str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 2048];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..count]);
                let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let headers = std::str::from_utf8(&request[..header_end]).unwrap();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if request.len() >= header_end + 4 + content_length {
                    assert!(headers.contains("Authorization: Bearer secret-test-token"));
                    assert!(headers.contains("ChatGPT-Account-Id: workspace-123"));
                    let body = &request[header_end + 4..header_end + 4 + content_length];
                    let json: serde_json::Value = serde_json::from_slice(body).unwrap();
                    assert_eq!(json["redeem_request_id"], "logical-attempt-1");
                    assert!(json.get("credit_id").is_none());
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}/consume"), worker)
    }

    #[test]
    fn reset_service_request_is_account_bound_and_idempotent() {
        let (endpoint, worker) = mock_reset_endpoint(r#"{"code":"reset","windows_reset":1}"#);
        let outcome =
            consume_rate_limit_reset_credit_at(&endpoint, &test_account(), "logical-attempt-1");
        assert_eq!(outcome, ResetCreditConsumeOutcome::Applied);
        worker.join().unwrap();
    }

    #[test]
    fn reset_service_no_credit_is_definitive_without_spend() {
        let (endpoint, worker) = mock_reset_endpoint(r#"{"code":"no_credit","windows_reset":0}"#);
        let outcome =
            consume_rate_limit_reset_credit_at(&endpoint, &test_account(), "logical-attempt-1");
        assert_eq!(
            outcome,
            ResetCreditConsumeOutcome::NotConsumed("no_credit".into())
        );
        worker.join().unwrap();
    }

    #[test]
    fn reset_service_rejects_mismatched_account_route() {
        let mut account = test_account();
        account.tokens.account_id = Some("different-workspace".into());
        assert_eq!(
            consume_rate_limit_reset_credit_at(
                "http://127.0.0.1:9/never-contact",
                &account,
                "logical-attempt-1"
            ),
            ResetCreditConsumeOutcome::Unavailable("active_account_route_mismatch".into())
        );
    }
}
