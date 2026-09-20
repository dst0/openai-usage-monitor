use crate::models::AccountConfig;
use std::io::Read;
use std::time::Duration;

#[path = "reset_credit_consume_response.rs"]
mod reset_credit_consume_response;

use reset_credit_consume_response::ResetCreditConsumeResponse;

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

pub(super) fn consume_rate_limit_reset_credit_at(
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
