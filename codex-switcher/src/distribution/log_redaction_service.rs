use super::log_redaction_structured_parser::sanitize_structured_values;
use super::log_redaction_token_service::{
    LogRedactionTokenService, ARG_MARKER, PATH_MARKER, TOKEN_MARKER,
};

pub struct LogRedactionService;

impl LogRedactionService {
    pub fn sanitize_text(value: &str) -> String {
        let value = LogRedactionTokenService::remove_controls(value);
        let value = sanitize_structured_values(&value);
        let mut output = Vec::new();
        let mut redact_next = false;
        for token in value.split_whitespace() {
            if redact_next {
                output.push(TOKEN_MARKER.to_string());
                redact_next = false;
                continue;
            }
            let clean = LogRedactionTokenService::sanitize_token(token);
            redact_next = token.eq_ignore_ascii_case("bearer")
                || token.split_once(['=', ':']).is_some_and(|(key, value)| {
                    key.eq_ignore_ascii_case("authorization")
                        && value.eq_ignore_ascii_case("bearer")
                });
            output.push(clean);
        }
        output.join(" ")
    }

    pub fn sanitize_field(field: &str, value: &str) -> String {
        let field = field
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_');
        let field = field.to_ascii_lowercase();
        let value = value.trim();
        match field.as_str() {
            "email" | "user_email" => Self::opaque_ref("email", value),
            "account" | "account_id" | "account_ref" | "app_target" | "cli_target" => {
                Self::opaque_ref("acct", value)
            }
            "display_name" | "name" | "nickname" => Self::opaque_ref("name", value),
            "thread" | "thread_id" => Self::opaque_ref("thread", value),
            "turn" | "turn_id" => Self::opaque_ref("turn", value),
            "session" | "session_id" => Self::opaque_ref("session", value),
            "operation" | "operation_id" | "op" | "op_id" => Self::operation_ref(value),
            "path" | "file" | "filename" => PATH_MARKER.to_string(),
            "token" | "access_token" | "refresh_token" | "authorization" | "api_key"
            | "openai_api_key" | "secret" | "password" | "credential" => TOKEN_MARKER.to_string(),
            "arg" | "argv" | "args" | "flag" => ARG_MARKER.to_string(),
            _ if value.contains('=') || value.contains(':') => TOKEN_MARKER.to_string(),
            _ => Self::sanitize_text(value),
        }
    }

    pub fn opaque_ref(namespace: &str, value: &str) -> String {
        if Self::is_opaque_ref(namespace, value) {
            return value.to_string();
        }
        let hash = Self::hash(namespace, value);
        format!("{namespace}_{hash:016x}")
    }

    pub fn operation_ref(value: &str) -> String {
        let generated = value
            .strip_prefix("op_dist_")
            .and_then(|rest| rest.split_once('_'))
            .is_some_and(|(timestamp, suffix)| {
                timestamp.len() == 13
                    && timestamp.bytes().all(|byte| byte.is_ascii_digit())
                    && suffix.len() == 12
                    && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        if generated {
            value.to_string()
        } else {
            Self::opaque_ref("op", value)
        }
    }

    fn hash(namespace: &str, value: &str) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in namespace
            .bytes()
            .chain([0].into_iter())
            .chain(value.as_bytes().iter().copied())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    fn is_opaque_ref(namespace: &str, value: &str) -> bool {
        let Some(hash) = value.strip_prefix(&format!("{namespace}_")) else {
            return false;
        };
        hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}
