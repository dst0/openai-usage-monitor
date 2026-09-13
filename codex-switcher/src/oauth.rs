use crate::models::{AuthTokens, OAuthTokenResponse};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::Value;

pub const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

pub fn extract_jwt_metadata(token: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(t) = token else {
        return (None, None);
    };
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() < 2 {
        return (None, None);
    }

    let payload_b64 = parts[1];
    // Add padding if needed
    let mut padded = payload_b64.to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }

    let Ok(decoded_bytes) = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&padded))
    else {
        return (None, None);
    };

    let Ok(json_val): Result<Value, _> = serde_json::from_slice(&decoded_bytes) else {
        return (None, None);
    };

    let email = json_val
        .get("email")
        .and_then(|v| v.as_str())
        .or_else(|| {
            json_val
                .get("https://api.openai.com/profile")
                .and_then(|p| p.get("email"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string());

    let plan = json_val
        .get("https://api.openai.com/auth")
        .and_then(|a| a.get("chatgpt_plan_type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    (email, plan)
}

pub fn extract_jwt_metadata_from_tokens(tokens: &AuthTokens) -> (Option<String>, Option<String>) {
    let (id_email, id_plan) = extract_jwt_metadata(tokens.id_token.as_deref());
    let (acc_email, acc_plan) = extract_jwt_metadata(Some(&tokens.access_token));

    let email = id_email.or(acc_email);
    let plan = id_plan.or(acc_plan);

    (email, plan)
}

pub fn refresh_access_token(tokens: &mut AuthTokens) -> Result<(), String> {
    let refresh_token = tokens
        .refresh_token
        .as_ref()
        .ok_or_else(|| "No refresh token available to refresh access token".to_string())?;

    let resp = match ureq::post(OAUTH_TOKEN_URL)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("User-Agent", "Codex/1.0")
        .send_form(&[
            ("client_id", OAUTH_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ]) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            if body.contains("refresh_token_invalidated")
                || body.contains("token_revoked")
                || body.contains("invalid_grant")
                || body.contains("Your session has ended")
            {
                return Err("Session ended (logged out in app). Re-login required.".to_string());
            }
            return Err(format!("OAuth token refresh failed with HTTP {}", code));
        }
        Err(e) => return Err(format!("OAuth token refresh request failed: {}", e)),
    };

    let token_resp: OAuthTokenResponse = resp
        .into_json()
        .map_err(|e| format!("Failed to parse OAuth token response: {}", e))?;

    tokens.access_token = token_resp.access_token;
    if let Some(new_refresh) = token_resp.refresh_token {
        tokens.refresh_token = Some(new_refresh);
    }
    if let Some(new_id) = token_resp.id_token {
        tokens.id_token = Some(new_id);
    }

    Ok(())
}
