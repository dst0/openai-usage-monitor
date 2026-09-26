use super::*;
use std::process::Command;
use std::time::Duration;

#[test]
fn returns_when_the_app_has_already_exited() {
    assert!(wait_for_app_exit_with(|| Ok(false), Duration::ZERO, Duration::ZERO,).unwrap());
}

#[test]
fn waits_until_the_app_exits() {
    let mut checks = 0;
    assert!(wait_for_app_exit_with(
        || {
            checks += 1;
            Ok(checks < 3)
        },
        Duration::from_secs(1),
        Duration::ZERO,
    )
    .unwrap());
    assert_eq!(checks, 3);
}

#[test]
fn times_out_without_forcing_termination() {
    assert!(!wait_for_app_exit_with(|| Ok(true), Duration::ZERO, Duration::ZERO,).unwrap());
}

#[test]
fn parses_only_the_exact_codex_app_executable() {
    let process_list = format!(
            "   0 kernel_task\n\
               1 /sbin/launchd\n\
              42 {CODEX_APP_EXECUTABLE}\n\
             43 /Applications/ChatGPT.app/Contents/Frameworks/Codex Helper.app/Contents/MacOS/Codex Helper\n\
             44 /Applications/Other.app/Contents/MacOS/ChatGPT\n"
        );

    assert_eq!(parse_codex_app_pids(&process_list).unwrap(), vec![42]);
}

#[test]
fn orphaned_bundled_writer_keeps_shared_auth_guard_closed() {
    let writer =
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex";
    let legacy_writer = "/Applications/ChatGPT.app/Contents/Resources/codex";
    let independent = "/usr/local/bin/codex";
    assert!(parse_shared_auth_activity(&format!("1 /sbin/launchd\n200 {writer}\n")).unwrap());
    assert!(
        parse_shared_auth_activity(&format!("1 /sbin/launchd\n201 {legacy_writer}\n")).unwrap()
    );
    assert!(!parse_shared_auth_activity(&format!("300 {independent}\n")).unwrap());
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
                extra: Default::default(),
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
                extra: Default::default(),
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
fn malformed_newest_rollout_event_cannot_authorize_unattended_dispatch() {
    let lines = vec![
        r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-a"}}"#
            .to_string(),
        r#"{"type":"event_msg","payload":{"type":"task_complete","error":{"message":"auth outage"}"#
            .to_string(),
    ];
    assert_eq!(
        inspect_thread_rollout_state_from_lines(&lines),
        ThreadRolloutState::Unknown
    );
}

#[test]
fn incomplete_final_rollout_line_is_not_a_valid_tail_state() {
    let root = std::env::temp_dir().join(format!(
        "codex-incomplete-rollout-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let tid = "01a07d3c-3008-75c2-87a6-2c5c75f0e499";
    let rollout = sessions.join(format!("rollout-{tid}.jsonl"));
    std::fs::write(
        &rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"",
    )
    .unwrap();
    assert_eq!(
        inspect_thread_rollout_state(&root, tid),
        ThreadRolloutState::Unknown
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_utf8_latest_rollout_record_cannot_authorize_unattended_dispatch() {
    let root = std::env::temp_dir().join(format!(
        "codex-invalid-utf8-rollout-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let tid = "01a07d3c-3008-75c2-87a6-2c5c75f0e499";
    let rollout = sessions.join(format!("rollout-{tid}.jsonl"));
    let mut bytes = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"ta".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b"sk_complete\",\"error\":{\"message\":\"auth outage\"}}}\n");
    std::fs::write(&rollout, bytes).unwrap();
    assert_eq!(
        inspect_thread_rollout_state(&root, tid),
        ThreadRolloutState::Unknown
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounded_tail_can_discard_a_split_multibyte_prefix_and_read_the_terminal_event() {
    let path = std::env::temp_dir().join(format!(
        "codex-tail-utf8-boundary-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let prefix = format!("{}\n", "é".repeat(100));
    let terminal = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n";
    let content = format!("{prefix}{terminal}");
    std::fs::write(&path, content.as_bytes()).unwrap();
    let cut_inside_last_character = prefix.len() - 2;
    let max_bytes = (content.len() - cut_inside_last_character) as u64;
    let lines = thread_rollout_inspector::read_rollout_tail_lines(&path, max_bytes);
    assert_eq!(
        thread_rollout_inspector::inspect_thread_rollout_state_from_lines(&lines),
        ThreadRolloutState::CleanCompleted
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn bounded_tail_keeps_a_complete_event_at_an_exact_newline_boundary() {
    let path = std::env::temp_dir().join(format!(
        "codex-tail-line-boundary-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let prefix = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n";
    let terminal = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n";
    std::fs::write(&path, format!("{prefix}{terminal}")).unwrap();
    let lines = thread_rollout_inspector::read_rollout_tail_lines(&path, terminal.len() as u64);
    assert_eq!(
        thread_rollout_inspector::inspect_thread_rollout_state_from_lines(&lines),
        ThreadRolloutState::CleanCompleted
    );
    std::fs::remove_file(path).unwrap();
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
fn detects_when_self_restart_needs_an_independent_worker() {
    let rows = format!("1 0 /sbin/launchd\n100 1 {CODEX_APP_EXECUTABLE}\n200 100 /app-server\n300 200 /bin/zsh\n400 300 /cxi\n500 1 /cxi");
    assert!(has_codex_ancestor(&rows, 400).unwrap());
    assert!(!has_codex_ancestor(&rows, 500).unwrap());
    assert!(has_codex_ancestor(&rows, 999).is_err());
}

#[test]
fn test_switch_to_account_rejects_relogin_needed() {
    let _home = crate::storage::test_codex_home::TestCodexHome::new("relogin-guard");

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
            extra: Default::default(),
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

    let mut active = acc.clone();
    active.id = "other@example.com:uuid-2".to_string();
    active.email = "other@example.com".to_string();
    active.account_id = "uuid-2".to_string();
    active.tokens.access_token = "at_2".to_string();
    active.tokens.refresh_token = Some("rt_2".to_string());
    active.tokens.account_id = Some("uuid-2".to_string());
    active.last_error = None;
    let file = crate::models::AccountsFile {
        active_account_id: Some("other@example.com:uuid-2".to_string()),
        settings: Default::default(),
        accounts: vec![acc, active.clone()],
    };
    crate::storage::save_accounts(&file).unwrap();
    crate::storage::write_active_auth_json(&crate::models::AuthJson {
        auth_mode: Some("chatgpt".to_string()),
        openai_api_key: None,
        tokens: Some(active.tokens),
        last_refresh: None,
        extra: Default::default(),
    })
    .unwrap();

    let auth_before = std::fs::read(crate::storage::auth_json_path()).unwrap();
    let registry_before = crate::storage::load_accounts().unwrap();
    // Fake process probes: the live process table must never decide a test.
    for desktop_running in [false, true] {
        let err = switch_to_account_with(
            "user@example.com:uuid-1",
            false,
            false,
            SwitchTrigger::User,
            || Ok(desktop_running),
            || Ok(desktop_running),
        )
        .unwrap_err();
        assert!(err.contains("requires re-login"), "{err}");
        assert!(err.contains("cxi relogin"));
    }
    assert_eq!(
        std::fs::read(crate::storage::auth_json_path()).unwrap(),
        auth_before
    );
    assert_eq!(
        crate::storage::load_accounts().unwrap().active_account_id,
        registry_before.active_account_id
    );
    assert!(!_home.path().join("direct-switch-journal.json").exists());
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
    let recent_error_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e405";

    let now = chrono::Utc::now().timestamp();
    let old_time = now - 4 * 86400; // 4 days ago
    let fresh_time = now - 300; // 5 minutes ago

    let database = root.join("state_5.sqlite");
    let sql = format!(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);\
             INSERT INTO threads VALUES ('{stale_id}', 0, 'user', {old_time}, '');\
             INSERT INTO threads VALUES ('{completed_id}', 0, 'user', {fresh_time}, '');\
             INSERT INTO threads VALUES ('{unknown_id}', 0, 'user', {fresh_time}, '');\
             INSERT INTO threads VALUES ('{recent_aborted_id}', 0, 'user', {fresh_time}, '');\
             INSERT INTO threads VALUES ('{recent_error_id}', 0, 'user', {fresh_time}, '');"
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

    create_rollout(
        recent_error_id,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t2","last_agent_message":null,"error":{"message":"unexpected status 401 Unauthorized","codex_error_info":"other"}}}"#,
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
            recent_error_id.to_string(),
        ],
    );

    assert_eq!(
        in_progress,
        vec![recent_aborted_id.to_string()],
        "stale, cleanly completed, unknown, and error-ended tasks must never be appended"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unit_tests_cannot_read_the_live_process_table() {
    // Every Desktop process probe funnels through one `/bin/ps` reader.
    let seam = "process table (/bin/ps)";
    crate::test_live_system::assert_forbidden(seam, is_codex_app_running_checked);
    crate::test_live_system::assert_forbidden(seam, is_shared_auth_active_checked);
    crate::test_live_system::assert_forbidden(seam, current_codex_app_pids);
    crate::test_live_system::assert_forbidden(
        seam,
        super::codex_process_probe::desktop_process_rows_checked,
    );
}
