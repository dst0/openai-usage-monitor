use super::*;
use std::process::Command;
use std::time::Duration;

#[test]
fn returns_when_the_app_has_already_exited() {
    assert!(wait_for_app_exit_with(
        || false,
        Duration::ZERO,
        Duration::ZERO,
    ));
}

#[test]
fn waits_until_the_app_exits() {
    let mut checks = 0;
    assert!(wait_for_app_exit_with(
        || {
            checks += 1;
            checks < 3
        },
        Duration::from_secs(1),
        Duration::ZERO,
    ));
    assert_eq!(checks, 3);
}

#[test]
fn times_out_without_forcing_termination() {
    assert!(!wait_for_app_exit_with(
        || true,
        Duration::ZERO,
        Duration::ZERO,
    ));
}

#[test]
fn parses_only_the_exact_codex_app_executable() {
    let process_list = format!(
            "  42 {CODEX_APP_EXECUTABLE}\n\
             43 /Applications/ChatGPT.app/Contents/Frameworks/Codex Helper.app/Contents/MacOS/Codex Helper\n\
             44 /Applications/Other.app/Contents/MacOS/ChatGPT\n"
        );

    assert_eq!(parse_codex_app_pids(&process_list), vec![42]);
}

#[test]
fn explicit_primary_is_always_first_and_never_duplicated() {
    let primary = "01a098c2-0fae-74d2-a80c-45d89e910e79".to_string();
    let mut targets = vec!["other".to_string(), primary.clone()];
    prioritize_primary(&mut targets, Some(&primary));
    assert_eq!(targets, [primary.clone(), "other".to_string()]);
    prioritize_primary(&mut targets, Some(&primary));
    assert_eq!(targets, [primary, "other".to_string()]);
}

#[test]
fn test_resolve_target_account_idx() {
    use crate::models::{AccountConfig, AuthTokens};

    let accounts = vec![
        AccountConfig {
            id: "user@example.com:3f533057-4bac-44ea".to_string(),
            name: Some("personal".to_string()),
            email: "user@example.com".to_string(),
            plan_type: "pro".to_string(),
            account_id: "3f533057-4bac-44ea".to_string(),
            tokens: AuthTokens {
                access_token: "tok1".to_string(),
                refresh_token: None,
                id_token: None,
                account_id: Some("3f533057-4bac-44ea".to_string()),
            },
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
        },
        AccountConfig {
            id: "dev@company.com:26a1ef5c-ad94-460e".to_string(),
            name: Some("work".to_string()),
            email: "dev@company.com".to_string(),
            plan_type: "team".to_string(),
            account_id: "26a1ef5c-ad94-460e".to_string(),
            tokens: AuthTokens {
                access_token: "tok2".to_string(),
                refresh_token: None,
                id_token: None,
                account_id: Some("26a1ef5c-ad94-460e".to_string()),
            },
            enabled: true,
            priority: 2,
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
        },
    ];

    // 1. Resolve by exact canonical ID
    assert_eq!(
        resolve_target_account_idx(&accounts, "user@example.com:3f533057-4bac-44ea"),
        Ok(0)
    );

    // 2. Resolve by nickname
    assert_eq!(resolve_target_account_idx(&accounts, "personal"), Ok(0));
    assert_eq!(resolve_target_account_idx(&accounts, "WORK"), Ok(1));

    // 3. Resolve by short UUID prefix
    assert_eq!(resolve_target_account_idx(&accounts, "3f5330"), Ok(0));
    assert_eq!(resolve_target_account_idx(&accounts, "26a1ef"), Ok(1));

    // 4. Resolve by email
    assert_eq!(
        resolve_target_account_idx(&accounts, "dev@company.com"),
        Ok(1)
    );

    // 5. Unknown account
    assert!(resolve_target_account_idx(&accounts, "unknown").is_err());
}

#[test]
fn test_clean_thread_id() {
    assert_eq!(
        clean_thread_id("01a07d3c-3008-75c2-87a6-2c5c75f0e48b"),
        "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
    );
    assert_eq!(
        clean_thread_id("codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b"),
        "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
    );
    assert_eq!(
        clean_thread_id("codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b/"),
        "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
    );
    assert_eq!(
        clean_thread_id("chatgpt://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b"),
        "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
    );
    assert_eq!(
        clean_thread_id("  codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b  "),
        "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
    );
}

#[test]
fn user_thread_lookup_fails_closed_for_missing_archived_and_subagent_rows() {
    let root = std::env::temp_dir().join(format!(
        "codex-user-thread-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    std::fs::create_dir_all(&root).unwrap();
    let user_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48b";
    let subagent_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48c";
    let archived_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48d";
    let missing_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48e";

    assert!(!is_user_thread(&root, user_id));
    let database = root.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT);\
             INSERT INTO threads VALUES ('{user_id}', 0, 'cli');\
             INSERT INTO threads VALUES ('{subagent_id}', 0, 'subagent');\
             INSERT INTO threads VALUES ('{archived_id}', 1, 'cli');"
    );
    let result = Command::new("/usr/bin/sqlite3")
        .arg(&database)
        .arg(sql)
        .status()
        .unwrap();
    assert!(result.success());
    assert!(is_user_thread(&root, user_id));
    assert!(!is_user_thread(&root, subagent_id));
    assert!(!is_user_thread(&root, archived_id));
    assert!(!is_user_thread(&root, missing_id));
    assert!(!is_user_thread(&root, "not-a-thread-id' OR 1=1 --"));

    let mut detected = vec![user_id.to_string()];
    append_eligible_pending(
        &root,
        &mut detected,
        vec![
            user_id.to_string(),
            subagent_id.to_string(),
            archived_id.to_string(),
            missing_id.to_string(),
        ],
    );
    assert_eq!(detected, vec![user_id.to_string()]);

    let mut primary_targets = Vec::new();
    assert!(!prioritize_primary_if_user(
        &root,
        &mut primary_targets,
        Some(&subagent_id.to_string())
    ));
    assert!(primary_targets.is_empty());
    assert!(prioritize_primary_if_user(
        &root,
        &mut primary_targets,
        Some(&user_id.to_string())
    ));
    assert_eq!(primary_targets, vec![user_id.to_string()]);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn test_rollout_clean_completed_with_trailing_events_and_null_rate_limits() {
    let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Audit project"}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":5.0},"rate_limit_reached_type":null}}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"Work is finished!","error":null}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-1"}}"#.to_string(),
            r#"{"type":"token_usage_record","payload":{"tokens":123}}"#.to_string(),
        ];

    let state = inspect_thread_rollout_state_from_lines(&lines);
    assert_eq!(state, ThreadRolloutState::CleanCompleted);
}

#[test]
fn test_rollout_interrupted_by_quota_exhaustion() {
    let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Run tests"}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-2","error":{"message":"You have hit your limit","codex_error_info":"usage_limit_exceeded"}}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-2"}}"#.to_string(),
        ];

    let state = inspect_thread_rollout_state_from_lines(&lines);
    assert_eq!(state, ThreadRolloutState::InterruptedByQuota);
}

#[test]
fn test_rollout_turn_aborted_by_user() {
    let lines = vec![
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"Investigate bug"}}"#
            .to_string(),
        r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#
            .to_string(),
        r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-3"}}"#
            .to_string(),
    ];

    let state = inspect_thread_rollout_state_from_lines(&lines);
    assert_eq!(state, ThreadRolloutState::TurnAborted);
}

#[test]
fn test_rollout_active_mid_turn() {
    let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Build feature"}}"#.to_string(),
            r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"cargo build"}}"#.to_string(),
        ];

    let state = inspect_thread_rollout_state_from_lines(&lines);
    assert_eq!(state, ThreadRolloutState::ActiveInProgress);
}

#[test]
fn test_inspect_thread_rollout_state_large_tail_fallback() {
    let root =
        std::env::temp_dir().join(format!("codex-rollout-large-tail-{}", std::process::id()));
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();

    let tid = "01a07d3c-3008-75c2-87a6-2c5c75f0e499";
    let rollout_file = sessions.join(format!("rollout-2026-09-19T00-00-00-{tid}.jsonl"));

    let mut content = String::new();
    content.push_str(r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":{"message":"usage_limit_exceeded"}}}"#);
    content.push('\n');

    // Pad with >130 KB of single-line tool completion output so 128KB tail seek starts inside it
    let huge_line = format!(
        r#"{{"type":"event_msg","payload":{{"type":"item_completed","thread_id":"{tid}","output":"{}"}}}}"#,
        "x".repeat(140_000)
    );
    content.push_str(&huge_line);
    content.push('\n');

    std::fs::write(&rollout_file, content).unwrap();

    let state = inspect_thread_rollout_state(&root, tid);
    assert_eq!(state, ThreadRolloutState::InterruptedByQuota);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn test_detect_in_progress_live() {
    let in_progress = detect_in_progress_threads();
    crate::runtime_print!("Live detected in-progress threads: {:?}", in_progress);
}

#[test]
fn detects_when_self_restart_needs_an_independent_worker() {
    let rows = format!("1 0 /sbin/launchd\n100 1 {CODEX_APP_EXECUTABLE}\n200 100 /app-server\n300 200 /bin/zsh\n400 300 /cxi\n500 1 /cxi");
    assert!(has_codex_ancestor(&rows, 400).unwrap());
    assert!(!has_codex_ancestor(&rows, 500).unwrap());
    assert!(has_codex_ancestor(&rows, 999).is_err());
}

#[test]
fn test_switch_to_account_rejects_relogin_needed() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir = std::env::temp_dir().join(format!("codex_relogin_guard_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    std::env::set_var("CODEX_HOME", &temp_dir);

    let acc = crate::models::AccountConfig {
        id: "user@example.com:uuid-1".to_string(),
        name: None,
        email: "user@example.com".to_string(),
        plan_type: "team".to_string(),
        account_id: "uuid-1".to_string(),
        tokens: crate::models::AuthTokens {
            access_token: "at_1".to_string(),
            refresh_token: Some("rt_1".to_string()),
            id_token: None,
            account_id: Some("uuid-1".to_string()),
        },
        enabled: true,
        priority: 1,
        last_primary_percentage: 100.0,
        last_reset_time: None,
        last_reset_after_seconds: None,
        last_weekly_percentage: None,
        last_weekly_reset_time: None,
        last_weekly_reset_after_seconds: None,
        last_credits: None,
        last_error: Some("401 Unauthorized (Session ended)".to_string()),
        last_checked: None,
        plan_multiplier: None,
        multiplier_is_manual: None,
        last_multiplier_checked: None,
        organization_name: None,
    };

    let file = crate::models::AccountsFile {
        active_account_id: Some("other@example.com:uuid-2".to_string()),
        settings: Default::default(),
        accounts: vec![acc],
    };
    crate::storage::save_accounts(&file).unwrap();

    let err = switch_to_account("user@example.com:uuid-1", false, false, SwitchTrigger::User)
        .unwrap_err();
    assert!(err.contains("requires re-login"));
    assert!(err.contains("cxi relogin"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_append_eligible_pending_rejects_stale_and_completed_tasks() {
    let root = std::env::temp_dir().join(format!(
        "codex-pending-filter-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();

    let stale_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e401";
    let completed_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e402";
    let unknown_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e403";
    let recent_aborted_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e404";

    let now = chrono::Utc::now().timestamp();
    let old_time = now - 4 * 86400; // 4 days ago
    let fresh_time = now - 300; // 5 minutes ago

    let database = root.join("state_5.sqlite");
    let sql = format!(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);\
             INSERT INTO threads VALUES ('{stale_id}', 0, 'user', {old_time}, '');\
             INSERT INTO threads VALUES ('{completed_id}', 0, 'user', {fresh_time}, '');\
             INSERT INTO threads VALUES ('{unknown_id}', 0, 'user', {fresh_time}, '');\
             INSERT INTO threads VALUES ('{recent_aborted_id}', 0, 'user', {fresh_time}, '');"
        );
    let result = Command::new("/usr/bin/sqlite3")
        .arg(&database)
        .arg(sql)
        .status()
        .unwrap();
    assert!(result.success());

    let create_rollout = |tid: &str, line: &str| {
        let path = sessions.join(format!("rollout-2026-09-19T00-00-00-{tid}.jsonl"));
        std::fs::write(&path, format!("{line}\n")).unwrap();
    };

    create_rollout(
        stale_id,
        r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
    );
    create_rollout(
        completed_id,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","error":null}}"#,
    );
    create_rollout(unknown_id, r#"{"type":"corrupted_event"}"#);
    create_rollout(
        recent_aborted_id,
        r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
    );

    let mut in_progress = Vec::new();
    append_eligible_pending(
        &root,
        &mut in_progress,
        vec![
            stale_id.to_string(),
            completed_id.to_string(),
            unknown_id.to_string(),
            recent_aborted_id.to_string(),
        ],
    );

    assert_eq!(
        in_progress,
        vec![recent_aborted_id.to_string()],
        "stale, cleanly completed, and unknown tasks must never be appended"
    );

    std::fs::remove_dir_all(root).unwrap();
}
