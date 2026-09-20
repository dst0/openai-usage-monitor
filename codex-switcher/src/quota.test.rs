use super::*;
use crate::models::AuthTokens;
use std::io::Read;
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
