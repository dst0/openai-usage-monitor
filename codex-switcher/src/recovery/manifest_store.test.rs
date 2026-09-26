use super::{
    manifest_store::{
        current_account_binding, finalize_target, load_manifest, load_ownerless_pending,
        load_pending, mark_dispatch_attempt_for_account, mark_dispatch_attempt_with_writer,
        prune_ineligible_targets_with, validate_target_account_binding, write_manifest,
    },
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
    restart_checkpoint_service::{
        cached_partial_capacity_for, confirmation_cursor_for, post_checkpoint_status,
        post_checkpoint_status_fresh_after_scan, save_pending, scanned_bytes_for,
    },
    stored_manifest::StoredManifest,
};
use crate::storage::test_codex_home::TestCodexHome;
use std::{path::PathBuf, process::Command};

#[test]
fn marker_write_failure_after_rename_restores_exact_checkpoint_before_ipc() {
    let env = crate::distribution::test_helper::TestEnv::new("marker_post_rename_error");
    let original = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e79".into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    };
    write_manifest(std::slice::from_ref(&original)).unwrap();
    let error = mark_dispatch_attempt_with_writer(
        &original.id,
        RecoveryMode::CapturedRestart,
        || Ok(()),
        |remaining| {
            write_manifest(remaining)?;
            Err("synthetic directory sync error after rename".into())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("synthetic directory sync error"));
    assert_eq!(load_manifest().unwrap(), vec![original]);
    drop(env);
}

fn write_ignored_records(writer: &mut impl std::io::Write, minimum_bytes: usize) {
    let record = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
        "x".repeat(8 * 1024)
    );
    for _ in 0..minimum_bytes.div_ceil(record.len()) {
        writer.write_all(record.as_bytes()).unwrap();
    }
}

fn no_op_line_of_length(length: usize) -> Vec<u8> {
    let mut line = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"}}".to_vec();
    assert!(length > line.len());
    line.resize(length - 1, b' ');
    line.push(b'\n');
    line
}

#[test]
fn restart_recovery_binds_to_committed_auth_before_cli_registry_updates() {
    let env = crate::distribution::test_helper::TestEnv::new("recovery_auth_transition");
    let old = crate::distribution::test_helper::make_account(
        "old",
        None,
        "old@example.com",
        "plus",
        0.0,
        None,
        0,
        None,
        None,
    );
    let target = crate::distribution::test_helper::make_account(
        "target",
        None,
        "target@example.com",
        "team",
        90.0,
        None,
        1,
        None,
        None,
    );
    env.populate(vec![old, target.clone()], Some("old"), Some("old"));
    let mut auth = crate::storage::read_active_auth_json().unwrap();
    auth.tokens = Some(target.tokens);
    crate::storage::write_active_auth_json(&auth).unwrap();
    let target_id = crate::storage::load_accounts()
        .unwrap()
        .accounts
        .into_iter()
        .find(|account| account.account_id == "target")
        .unwrap()
        .id;
    assert_eq!(
        current_account_binding().as_deref(),
        Some(target_id.as_str())
    );
}

#[test]
fn conflicting_token_emails_do_not_bind_a_recovery_account() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let env = crate::distribution::test_helper::TestEnv::new("recovery_conflicting_email");
    let target = crate::distribution::test_helper::make_account(
        "target",
        None,
        "target@example.com",
        "team",
        90.0,
        None,
        0,
        None,
        None,
    );
    env.populate(vec![target.clone()], Some("target"), Some("target"));
    let mut auth = crate::storage::read_active_auth_json().unwrap();
    let tokens = auth.tokens.as_mut().unwrap();
    let target_claim = URL_SAFE_NO_PAD.encode(r#"{"email":"target@example.com"}"#);
    let other_claim = URL_SAFE_NO_PAD.encode(r#"{"email":"other@example.com"}"#);
    tokens.id_token = Some(format!("header.{target_claim}.signature"));
    tokens.access_token = format!("header.{other_claim}.signature");
    crate::storage::write_active_auth_json(&auth).unwrap();
    assert_eq!(current_account_binding(), None);
}
#[test]
fn unreadable_thread_index_does_not_erase_deferred_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-index-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let target = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let mut pending = vec![target.clone()];
    assert!(prune_ineligible_targets_with(&home, &mut pending, |_| {
        Err("SQLite temporarily unavailable".into())
    })
    .is_err());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].offset, Some(42));
    assert_eq!(pending[0].owner_account_id.as_deref(), Some("account-a"));

    // An unknown rollout is retained only for an already journaled retry;
    // it is never sufficient evidence to create a new recovery request.
    let now = chrono::Utc::now().timestamp();
    prune_ineligible_targets_with(&home, &mut pending, |_| Ok(Some(now))).unwrap();
    assert_eq!(pending.len(), 1);
    pending[0].awaiting_owner = false;
    prune_ineligible_targets_with(&home, &mut pending, |_| Ok(Some(now))).unwrap();
    assert!(pending.is_empty());
    let mut archived = vec![target];
    prune_ineligible_targets_with(&home, &mut archived, |_| Ok(None)).unwrap();
    assert!(archived.is_empty());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn legacy_pending_manifest_remains_readable() {
    let json = r#"["01a098c2-0fae-74d2-a80c-45d89e910e79"]"#;
    let stored: StoredManifest = serde_json::from_str(json).unwrap();
    let StoredManifest::Legacy(ids) = stored else {
        panic!("legacy manifest was not recognized")
    };
    assert_eq!(ids, ["01a098c2-0fae-74d2-a80c-45d89e910e79"]);
}

#[test]
fn ownerless_retry_survives_journal_round_trip_without_enabling_old_entries() {
    let old =
        r#"{"version":1,"targets":[{"id":"01a098c2-0fae-74d2-a80c-45d89e910e79","offset":42}]}"#;
    let StoredManifest::Current(old_manifest) =
        serde_json::from_str::<StoredManifest>(old).unwrap()
    else {
        panic!("current manifest was not recognized")
    };
    assert!(!old_manifest.targets[0].awaiting_owner);

    let mut pending = old_manifest.targets;
    pending[0].awaiting_owner = true;
    let encoded = serde_json::to_vec(&super::pending_manifest::PendingManifest {
        version: 1,
        targets: pending,
    })
    .unwrap();
    let StoredManifest::Current(reloaded) = serde_json::from_slice(&encoded).unwrap() else {
        panic!("ownerless manifest was not recognized")
    };
    assert!(reloaded.targets[0].awaiting_owner);
    assert_eq!(reloaded.targets[0].offset, Some(42));
}

#[test]
fn only_failed_pre_dispatch_owner_resolution_keeps_a_retry_checkpoint() {
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let entry = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    };
    let mut no_owner = vec![entry.clone()];
    finalize_target(&mut no_owner, id, true, false, Some("account-a"), false);
    assert!(no_owner[0].awaiting_owner);
    assert_eq!(no_owner[0].owner_account_id.as_deref(), Some("account-a"));
    assert_eq!(no_owner[0].offset, Some(42));

    let mut unknown_ipc_outcome = vec![entry.clone()];
    finalize_target(
        &mut unknown_ipc_outcome,
        id,
        true,
        true,
        Some("account-a"),
        false,
    );
    assert!(unknown_ipc_outcome.is_empty());

    let mut unrelated_failure = vec![entry];
    finalize_target(
        &mut unrelated_failure,
        id,
        false,
        false,
        Some("account-a"),
        false,
    );
    assert!(unrelated_failure.is_empty());

    let mut account_changed = no_owner;
    finalize_target(
        &mut account_changed,
        id,
        false,
        false,
        Some("account-b"),
        true,
    );
    assert_eq!(
        account_changed[0].owner_account_id.as_deref(),
        Some("account-a")
    );
}

#[test]
fn wrong_account_cannot_recheckpoint_or_rebind_an_ownerless_target() {
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let entry = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let targets = vec![entry];
    assert!(validate_target_account_binding(&targets, &[id.into()], Some("account-b")).is_err());
    assert!(validate_target_account_binding(&targets, &[id.into()], None).is_err());
    assert!(validate_target_account_binding(&targets, &[id.into()], Some("account-a")).is_ok());
    assert_eq!(targets[0].offset, Some(42));
    assert_eq!(targets[0].owner_account_id.as_deref(), Some("account-a"));
}

#[test]
fn dispatch_marker_is_durable_before_any_ipc_send() {
    let _home = TestCodexHome::new("dispatch-marker");
    let first = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let other = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    write_manifest(&[
        PendingTarget {
            id: first.into(),
            offset: Some(42),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        },
        PendingTarget {
            id: other.into(),
            offset: Some(84),
            awaiting_owner: true,
            captured_restart: false,
            owner_account_id: Some("account-a".into()),
        },
    ])
    .unwrap();
    assert!(mark_dispatch_attempt_for_account(
        first,
        Some("account-b"),
        RecoveryMode::DeferredCaptured
    )
    .is_err());
    assert_eq!(load_manifest().unwrap().len(), 2);
    mark_dispatch_attempt_for_account(first, Some("account-a"), RecoveryMode::DeferredCaptured)
        .unwrap();
    // A fresh process reload sees only the other target, even if the first
    // process crashed immediately before or after its IPC write.
    let reloaded = load_manifest().unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, other);
    assert!(reloaded[0].awaiting_owner);
    assert!(mark_dispatch_attempt_for_account(
        first,
        Some("account-a"),
        RecoveryMode::DeferredCaptured
    )
    .is_err());
}

#[test]
fn new_restart_preserves_an_older_ownerless_checkpoint_and_account() {
    let _home = TestCodexHome::new("ownerless-preserve");
    let old = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let new = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    write_manifest(&[PendingTarget {
        id: old.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    }])
    .unwrap();
    save_pending(&[new.into()]).unwrap();
    let pending = load_manifest().unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].id, old);
    assert_eq!(pending[0].offset, Some(42));
    assert_eq!(pending[0].owner_account_id.as_deref(), Some("account-a"));
    assert!(pending[1].captured_restart);
    assert!(!pending[1].awaiting_owner);
    assert_eq!(load_ownerless_pending().unwrap(), [old]);
}

#[test]
fn second_restart_checkpoint_excludes_shutdown_events() {
    let test_home = TestCodexHome::new("recheckpoint");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"initial event\n").unwrap();

    save_pending(&[id.into()]).unwrap();
    let first = load_manifest().unwrap()[0].offset.unwrap();
    assert_eq!(first, 14);
    std::fs::write(&rollout, b"initial event\nshutdown event\n").unwrap();
    save_pending(&[id.into()]).unwrap();
    let second = load_manifest().unwrap()[0].offset.unwrap();
    assert_eq!(second, std::fs::metadata(&rollout).unwrap().len());
    assert!(second > first);
}

#[test]
fn new_turn_needs_work_before_replacing_stale_ownerless_checkpoint() {
    let test_home = TestCodexHome::new("new-turn");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let old_offset = std::fs::metadata(&rollout).unwrap().len();
    write_manifest(&[PendingTarget {
        id: id.into(),
        offset: Some(old_offset),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    }])
    .unwrap();

    let metadata = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\"}}\n";
    std::fs::write(&rollout, [b"checkpoint\n".as_slice(), metadata].concat()).unwrap();
    save_pending(&[id.into()]).unwrap();
    let unchanged = load_manifest().unwrap();
    assert!(unchanged[0].awaiting_owner);
    assert_eq!(unchanged[0].offset, Some(old_offset));
    assert_eq!(
        unchanged[0].owner_account_id.as_deref(),
        Some("old-account")
    );

    let started = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n";
    std::fs::write(
        &rollout,
        [b"checkpoint\n".as_slice(), metadata, started].concat(),
    )
    .unwrap();
    save_pending(&[id.into()]).unwrap();
    let recaptured = load_manifest().unwrap();
    assert!(recaptured[0].awaiting_owner);
    assert_eq!(
        recaptured[0].owner_account_id.as_deref(),
        Some("old-account")
    );
    assert_eq!(recaptured[0].offset, Some(old_offset));

    let work = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\"}}\n";
    let failed = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"new-turn\",\"error\":{\"codex_error_info\":\"usage_limit_exceeded\"}}}\n";
    std::fs::write(
        &rollout,
        [b"checkpoint\n".as_slice(), metadata, started, work, failed].concat(),
    )
    .unwrap();
    save_pending(&[id.into()]).unwrap();
    let failed_turn = load_manifest().unwrap();
    assert!(failed_turn[0].awaiting_owner);
    assert_eq!(
        failed_turn[0].owner_account_id.as_deref(),
        Some("old-account")
    );

    std::fs::write(
        &rollout,
        [b"checkpoint\n".as_slice(), metadata, started, work].concat(),
    )
    .unwrap();
    save_pending(&[id.into()]).unwrap();
    let recaptured = load_manifest().unwrap();
    assert!(!recaptured[0].awaiting_owner);
    assert_eq!(recaptured[0].owner_account_id, None);
    assert_eq!(
        recaptured[0].offset,
        Some(std::fs::metadata(&rollout).unwrap().len())
    );
}

#[test]
fn queued_follow_up_preserves_old_ownerless_checkpoint_after_new_work() {
    let test_home = TestCodexHome::new("queued-checkpoint");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let old_offset = std::fs::metadata(&rollout).unwrap().len();
    write_manifest(&[PendingTarget {
        id: id.into(),
        offset: Some(old_offset),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    }])
    .unwrap();
    let queue = home.join("queue_1.sqlite");
    let create = format!(
        "CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');"
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(create)
        .status()
        .unwrap()
        .success());
    let replacement = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\"}}\n";
    std::fs::write(&rollout, [b"checkpoint\n".as_slice(), replacement].concat()).unwrap();

    save_pending(&[id.into()]).unwrap();
    let retained = load_manifest().unwrap();
    assert!(retained[0].awaiting_owner);
    assert_eq!(retained[0].owner_account_id.as_deref(), Some("old-account"));
    assert_eq!(retained[0].offset, Some(old_offset));

    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(format!(
            "DELETE FROM queued_items WHERE thread_id = '{id}';"
        ))
        .status()
        .unwrap()
        .success());
    save_pending(&[id.into()]).unwrap();
    let replaced = load_manifest().unwrap();
    assert!(!replaced[0].awaiting_owner);
    assert_eq!(replaced[0].owner_account_id, None);
}

#[test]
fn long_rollout_after_old_checkpoint_still_supersedes_stale_binding() {
    let test_home = TestCodexHome::new("long-checkpoint");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    write_manifest(&[PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    }])
    .unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    write_ignored_records(&mut writer, 32 * 1024 * 1024);
    std::io::Write::write_all(
        &mut writer,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\"}}\n",
    )
    .unwrap();
    let mut retained_during_scan = false;
    for _ in 0..8 {
        save_pending(&[id.into()]).unwrap();
        if !load_manifest().unwrap()[0].awaiting_owner {
            break;
        }
        retained_during_scan = true;
    }
    assert!(retained_during_scan);
    let updated = load_manifest().unwrap();
    assert!(!updated[0].awaiting_owner);
    assert_eq!(updated[0].owner_account_id, None);
}

fn rewrite_middle_after_cached_proof(rollout: &std::path::Path) {
    use std::io::Write;

    let original = std::fs::read(rollout).unwrap();
    let mut replacement = original.clone();
    for old in [
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n"
            .as_slice(),
        b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n"
            .as_slice(),
    ] {
        let start = original
            .windows(old.len())
            .position(|window| window == old)
            .unwrap();
        replacement[start..start + old.len()].copy_from_slice(&no_op_line_of_length(old.len()));
    }
    std::fs::write(rollout, replacement).unwrap();
    let rewritten = std::fs::read(rollout).unwrap();
    assert_eq!(&original[11..11 + 64], &rewritten[11..11 + 64]);
    assert_eq!(
        &original[original.len() - 64..],
        &rewritten[rewritten.len() - 64..]
    );
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(rollout)
        .unwrap();
    file.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"}}\n")
        .unwrap();
}

fn cached_proof_fixture(home: &std::path::Path, id: &str) -> (PathBuf, PendingTarget) {
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    let mut content = b"checkpoint\n".to_vec();
    let ignored = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
        "x".repeat(65)
    );
    content.extend(ignored.as_bytes());
    content.extend(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n");
    content.extend(b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n");
    content.extend(ignored.as_bytes());
    std::fs::write(&rollout, content).unwrap();
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    };
    assert_eq!(post_checkpoint_status(home, &target), Some((true, true)));
    (rollout, target)
}

#[test]
fn middle_only_rewrite_cannot_replace_old_ownerless_checkpoint() {
    let test_home = TestCodexHome::new("middle-save");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let (rollout, target) = cached_proof_fixture(&home, id);
    write_manifest(std::slice::from_ref(&target)).unwrap();
    rewrite_middle_after_cached_proof(&rollout);

    save_pending(&[id.into()]).unwrap();
    let remaining = load_manifest().unwrap();
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].awaiting_owner);
    assert_eq!(remaining[0].offset, target.offset);
    assert_eq!(remaining[0].owner_account_id, target.owner_account_id);
}

#[test]
fn rewrite_during_bounded_confirmation_restarts_full_proof() {
    let test_home = TestCodexHome::new("middle-rescan");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let (rollout, target) = cached_proof_fixture(&home, id);
    write_manifest(std::slice::from_ref(&target)).unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    write_ignored_records(&mut writer, 10 * 1024 * 1024);
    drop(writer);

    save_pending(&[id.into()]).unwrap();
    assert!(load_manifest().unwrap()[0].awaiting_owner);
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, true)));
    let confirmation_cursor = confirmation_cursor_for(&rollout, 11).unwrap();
    assert!(confirmation_cursor > 11);
    assert!(confirmation_cursor < std::fs::metadata(&rollout).unwrap().len());
    rewrite_middle_after_cached_proof(&rollout);
    save_pending(&[id.into()]).unwrap();
    let remaining = load_manifest().unwrap();
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].awaiting_owner);
    assert_eq!(remaining[0].owner_account_id, target.owner_account_id);
}

#[test]
fn foreground_prepare_rechecks_middle_only_rewrite_from_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-middle-foreground-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let (rollout, _) = cached_proof_fixture(&home, id);
    let database = home.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, rollout_path TEXT); INSERT INTO threads VALUES ('{id}', 0, 'user', '{}');",
        rollout.display()
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&database)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    rewrite_middle_after_cached_proof(&rollout);

    let target = super::recovery_target::prepare_target(&home, id, Some(11), 1024 * 1024)
        .unwrap()
        .unwrap();
    assert!(target.scan_complete);
    assert!(!target.observer.evidence.verified(None));
    assert!(!target.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn middle_only_rewrite_cannot_prune_old_ownerless_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-middle-prune-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let (rollout, target) = cached_proof_fixture(&home, id);
    rewrite_middle_after_cached_proof(&rollout);
    let mut targets = vec![target];
    prune_ineligible_targets_with(&home, &mut targets, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(targets.len(), 1);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn completed_tail_before_checkpoint_does_not_erase_ownerless_retry() {
    let home = std::env::temp_dir().join(format!("codex-old-complete-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"old-turn\",\"error\":null}}\n",
    )
    .unwrap();
    let offset = std::fs::metadata(&rollout).unwrap().len();
    let mut targets = vec![PendingTarget {
        id: id.into(),
        offset: Some(offset),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    }];
    prune_ineligible_targets_with(&home, &mut targets, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(targets.len(), 1);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn prune_rotates_one_ownerless_rollout_scan_per_pass() {
    let home = std::env::temp_dir().join(format!("codex-prune-rotation-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let mut targets = Vec::new();
    let mut rollouts = Vec::new();
    for index in 1..=3 {
        let id = format!("01a098c2-0fae-74d2-a80c-{index:012x}");
        let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
        let mut contents = b"checkpoint\n".to_vec();
        contents.extend(vec![b'x'; 1024 * 1024]);
        std::fs::write(&rollout, contents).unwrap();
        rollouts.push(rollout);
        targets.push(PendingTarget {
            id,
            offset: Some(11),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        });
    }
    for expected_scanned in 1..=3 {
        prune_ineligible_targets_with(&home, &mut targets, |_| {
            Ok(Some(chrono::Utc::now().timestamp()))
        })
        .unwrap();
        assert_eq!(targets.len(), 3);
        assert_eq!(
            rollouts
                .iter()
                .filter(|path| scanned_bytes_for(path, 11).unwrap_or(0) > 0)
                .count(),
            expected_scanned,
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn ownerless_rotation_is_fair_when_other_homes_are_probed() {
    let root = std::env::temp_dir().join(format!("codex-prune-interleave-{}", std::process::id()));
    let primary = root.join("primary");
    let other = root.join("other");
    std::fs::create_dir_all(primary.join("sessions")).unwrap();
    std::fs::create_dir_all(other.join("sessions")).unwrap();
    let mut primary_targets = Vec::new();
    let mut primary_rollouts = Vec::new();
    for index in 1..=3 {
        let id = format!("01a098c2-0fae-74d2-a80c-{index:012x}");
        let rollout = primary
            .join("sessions")
            .join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
        std::fs::write(&rollout, b"checkpoint\nmetadata\n").unwrap();
        primary_rollouts.push(rollout);
        primary_targets.push(PendingTarget {
            id,
            offset: Some(11),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("owner".into()),
        });
    }
    let other_id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let other_rollout = other
        .join("sessions")
        .join(format!("rollout-2026-09-26T00-00-00-{other_id}.jsonl"));
    std::fs::write(&other_rollout, b"checkpoint\nmetadata\n").unwrap();
    let mut other_targets = vec![PendingTarget {
        id: other_id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("owner".into()),
    }];
    for _ in 0..3 {
        prune_ineligible_targets_with(&primary, &mut primary_targets, |_| {
            Ok(Some(chrono::Utc::now().timestamp()))
        })
        .unwrap();
        for _ in 0..2 {
            prune_ineligible_targets_with(&other, &mut other_targets, |_| {
                Ok(Some(chrono::Utc::now().timestamp()))
            })
            .unwrap();
        }
    }
    assert_eq!(primary_targets.len(), 3);
    assert_eq!(
        primary_rollouts
            .iter()
            .filter(|path| scanned_bytes_for(path, 11).unwrap_or(0) > 0)
            .count(),
        3,
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ownerless_prune_inspects_only_the_selected_tail() {
    use super::manifest_prune_service::ManifestPruneService;
    let home = std::env::temp_dir().join(format!("codex-prune-tail-{}", std::process::id()));
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let mut targets = Vec::new();
    let mut rollouts = Vec::new();
    for index in 1..=3 {
        let id = format!("01a098c2-0fae-74d2-a80c-{index:012x}");
        let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
        let mut contents = b"checkpoint\n".to_vec();
        contents.extend(vec![b'x'; 1024 * 1024]);
        std::fs::write(&rollout, contents).unwrap();
        rollouts.push(rollout);
        targets.push(PendingTarget {
            id,
            offset: Some(11),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("owner".into()),
        });
    }
    let mut tail_reads = 0;
    ManifestPruneService::run_with_inspector(
        &home,
        &mut targets,
        |_| Ok(Some(chrono::Utc::now().timestamp())),
        |_, _| {
            tail_reads += 1;
            crate::switcher::ThreadRolloutState::CleanCompleted
        },
    )
    .unwrap();
    assert_eq!(targets.len(), 3);
    assert_eq!(tail_reads, 1, "ownerless pruning must not read every tail");
    assert_eq!(
        rollouts
            .iter()
            .filter(|path| scanned_bytes_for(path, 11).unwrap_or(0) > 0)
            .count(),
        1,
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn manually_started_turn_retires_old_ownerless_retry() {
    let home = std::env::temp_dir().join(format!("codex-manual-prune-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n",
    )
    .unwrap();
    let mut targets = vec![PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    }];
    prune_ineligible_targets_with(&home, &mut targets, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(targets.len(), 1, "a start alone is not resumed work");

    let mut rollout_writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    std::io::Write::write_all(
        &mut rollout_writer,
        b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n",
    )
    .unwrap();
    prune_ineligible_targets_with(&home, &mut targets, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert!(targets.is_empty());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_new_turn_does_not_retire_deferred_retry_as_verified_work() {
    let home = std::env::temp_dir().join(format!("codex-failed-manual-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"new-turn\",\"error\":{\"codex_error_info\":\"usage_limit_exceeded\"}}}\n",
    )
    .unwrap();
    let mut targets = vec![PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    }];
    prune_ineligible_targets_with(&home, &mut targets, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(targets.len(), 1);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn incomplete_failed_turn_cannot_retire_ownerless_checkpoint() {
    let test_home = TestCodexHome::new("partial-failure");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let old = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    };
    write_manifest(std::slice::from_ref(&old)).unwrap();
    let started = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n";
    let work = b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n";
    let partial_failure = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"new-turn\",\"error\":";
    std::fs::write(
        &rollout,
        [b"checkpoint\n".as_slice(), started, work, partial_failure].concat(),
    )
    .unwrap();

    save_pending(&[id.into()]).unwrap();
    let saved = load_manifest().unwrap();
    assert!(saved[0].awaiting_owner);
    assert_eq!(saved[0].offset, old.offset);
    assert_eq!(saved[0].owner_account_id, old.owner_account_id);
    let mut pruned = saved;
    prune_ineligible_targets_with(&home, &mut pruned, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(pruned.len(), 1);

    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    use std::io::Write;
    writer
        .write_all(b"{\"codex_error_info\":\"usage_limit_exceeded\"}}}\n")
        .unwrap();
    drop(writer);
    save_pending(&[id.into()]).unwrap();
    assert!(load_manifest().unwrap()[0].awaiting_owner);
    prune_ineligible_targets_with(&home, &mut pruned, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(pruned.len(), 1);
}

#[test]
fn oversized_completed_failure_cannot_retire_ownerless_checkpoint() {
    let test_home = TestCodexHome::new("oversized-failure");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let old = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    };
    write_manifest(std::slice::from_ref(&old)).unwrap();
    let started = serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"new-turn"}});
    let work = serde_json::json!({"type":"response_item","payload":{"type":"reasoning"}});
    let failure = serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"new-turn","error":{"codex_error_info":"usage_limit_exceeded","padding":"x".repeat(super::observer::MAX_LINE)}}});
    let mut bytes = b"checkpoint\n".to_vec();
    for event in [started, work, failure] {
        bytes.extend_from_slice(event.to_string().as_bytes());
        bytes.push(b'\n');
    }
    std::fs::write(&rollout, bytes).unwrap();

    save_pending(&[id.into()]).unwrap();
    let saved = load_manifest().unwrap();
    assert_eq!(saved.len(), 1);
    assert!(saved[0].awaiting_owner);
    assert_eq!(saved[0].offset, old.offset);
    assert_eq!(saved[0].owner_account_id, old.owner_account_id);
    let mut pruned = vec![old];
    prune_ineligible_targets_with(&home, &mut pruned, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(pruned.len(), 1);
}

#[test]
fn new_agent_work_does_not_discard_an_existing_queued_follow_up() {
    let home = std::env::temp_dir().join(format!("codex-manual-queue-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n",
    )
    .unwrap();
    let queue = home.join("queue_1.sqlite");
    let sql = format!(
        "CREATE TABLE queued_items (thread_id TEXT, payload TEXT); INSERT INTO queued_items VALUES ('{id}', 'restart-paused');"
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    let mut targets = vec![PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    }];
    prune_ineligible_targets_with(&home, &mut targets, |_| {
        Ok(Some(chrono::Utc::now().timestamp()))
    })
    .unwrap();
    assert_eq!(targets.len(), 1);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn unchanged_ownerless_rollout_is_not_rescanned_on_every_probe() {
    let home = std::env::temp_dir().join(format!("codex-incremental-scan-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    write_ignored_records(&mut writer, 8 * 1024 * 1024);
    std::io::Write::write_all(
        &mut writer,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n",
    )
    .unwrap();
    drop(writer);
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, false)));
    let first_bytes = scanned_bytes_for(&rollout, 11).unwrap();
    assert!(first_bytes >= 8 * 1024 * 1024);
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, false)));
    assert_eq!(scanned_bytes_for(&rollout, 11), Some(first_bytes));

    let work = b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n";
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    std::io::Write::write_all(&mut writer, work).unwrap();
    drop(writer);
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, true)));
    assert_eq!(
        scanned_bytes_for(&rollout, 11),
        Some(first_bytes + work.len() as u64)
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn foreground_recovery_rechecks_the_checkpoint_after_deferred_scan() {
    let home = std::env::temp_dir().join(format!("codex-cache-foreground-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    write_ignored_records(&mut writer, 8 * 1024 * 1024);
    std::io::Write::write_all(
        &mut writer,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n",
    )
    .unwrap();
    drop(writer);
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, false)));
    let scanned_before_handoff = scanned_bytes_for(&rollout, 11).unwrap();
    let database = home.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, rollout_path TEXT); INSERT INTO threads VALUES ('{id}', 0, 'user', '{}');",
        rollout.display()
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&database)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    let prepared = super::recovery_target::prepare_target(&home, id, Some(11), 16 * 1024 * 1024)
        .unwrap()
        .unwrap();
    let mut observer = prepared.observer;
    assert_eq!(observer.offset, std::fs::metadata(&rollout).unwrap().len());
    assert!(observer.evidence.started);
    let work = b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n";
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    std::io::Write::write_all(&mut writer, work).unwrap();
    drop(writer);
    observer
        .poll_to(std::fs::metadata(&rollout).unwrap().len())
        .unwrap();
    assert!(observer.evidence.verified(None));
    assert_eq!(
        scanned_bytes_for(&rollout, 11),
        Some(scanned_before_handoff)
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn checkpoint_cursor_resets_after_rollout_replacement_or_truncation() {
    let home = std::env::temp_dir().join(format!("codex-cache-replace-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n",
    )
    .unwrap();
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, true)));

    let replacement = sessions.join("replacement.jsonl");
    std::fs::write(&replacement, b"checkpoint\nmetadata\n").unwrap();
    std::fs::rename(&replacement, &rollout).unwrap();
    assert_eq!(post_checkpoint_status(&home, &target), None);

    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    assert_eq!(post_checkpoint_status(&home, &target), Some((false, false)));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn appended_fragment_completes_one_lifecycle_record() {
    let home = std::env::temp_dir().join(format!("codex-cache-partial-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_sta",
    )
    .unwrap();
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    assert_eq!(post_checkpoint_status(&home, &target), Some((false, false)));
    let first_bytes = scanned_bytes_for(&rollout, 11).unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    std::io::Write::write_all(&mut writer, b"rted\",\"turn_id\":\"new-turn\"}}\n").unwrap();
    drop(writer);
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, false)));
    assert!(scanned_bytes_for(&rollout, 11).unwrap() > first_bytes);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn long_snapshot_scans_in_bounded_chunks_before_reporting_evidence() {
    let home = std::env::temp_dir().join(format!("codex-scan-budget-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, b"checkpoint\n").unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&rollout)
        .unwrap();
    write_ignored_records(&mut writer, 32 * 1024 * 1024);
    std::io::Write::write_all(
        &mut writer,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n",
    )
    .unwrap();
    drop(writer);
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    assert_eq!(post_checkpoint_status(&home, &target), None);
    assert!(scanned_bytes_for(&rollout, 11).unwrap() <= 16 * 1024 * 1024);
    assert_eq!(post_checkpoint_status(&home, &target), None);
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, false)));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn oversized_lines_do_not_pin_buffers_across_cold_targets() {
    let home = std::env::temp_dir().join(format!("codex-scan-memory-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    for index in 1..=12 {
        let id = format!("01a098c2-0fae-74d2-a80c-{index:012x}");
        let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
        let mut bytes = b"checkpoint\n".to_vec();
        bytes.extend(vec![b'x'; 131_073]);
        std::fs::write(&rollout, bytes).unwrap();
        let target = PendingTarget {
            id,
            offset: Some(11),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        };
        assert_eq!(post_checkpoint_status(&home, &target), None);
        assert!(cached_partial_capacity_for(&rollout, 11).unwrap() < 1024);
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn same_inode_rewrite_with_preserved_boundary_invalidates_evidence() {
    use std::os::unix::fs::MetadataExt;

    let home = std::env::temp_dir().join(format!("codex-scan-rewrite-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    let mut original = b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n".to_vec();
    let ignored = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
        "x".repeat(63)
    );
    original.extend(ignored.as_bytes());
    std::fs::write(&rollout, &original).unwrap();
    let before = std::fs::metadata(&rollout).unwrap();
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    assert_eq!(post_checkpoint_status(&home, &target), Some((true, true)));

    let mut rewritten = original.clone();
    for old in [
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n"
            .as_slice(),
        b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n"
            .as_slice(),
    ] {
        let start = original.windows(old.len()).position(|line| line == old).unwrap();
        rewritten[start..start + old.len()].copy_from_slice(&no_op_line_of_length(old.len()));
    }
    rewritten.extend(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"}}\n");
    std::fs::write(&rollout, rewritten).unwrap();
    assert_eq!(std::fs::metadata(&rollout).unwrap().ino(), before.ino());
    assert_eq!(post_checkpoint_status(&home, &target), Some((false, false)));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn load_pending_skips_ownerless_targets_without_a_thread_index() {
    let _home = TestCodexHome::new("skip-ownerless");
    let target = PendingTarget {
        id: "01a098c2-0fae-74d2-a80c-45d89e910e79".into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    write_manifest(&[target]).unwrap();
    assert!(load_pending().unwrap().is_empty());
    assert_eq!(load_ownerless_pending().unwrap().len(), 1);
}

#[test]
fn manifest_cleared_when_targets_empty() {
    let test_home = TestCodexHome::new("manifest-clear-test");
    let temp_dir = test_home.path().to_path_buf();

    let targets = vec![PendingTarget {
        id: "01a07d3c-3008-75c2-87a6-2c5c75f0e401".to_string(),
        offset: Some(123),
        awaiting_owner: false,
        captured_restart: false,
        owner_account_id: None,
    }];
    assert!(write_manifest(&targets).is_ok());
    let manifest_path = temp_dir.join("desktop-recovery.json");
    assert!(manifest_path.exists());
    let loaded = load_manifest().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "01a07d3c-3008-75c2-87a6-2c5c75f0e401");

    // When write_manifest is called with empty targets, the file should be deleted
    assert!(write_manifest(&[]).is_ok());
    assert!(!manifest_path.exists());
    let reloaded = load_manifest().unwrap();
    assert!(reloaded.is_empty());
}

#[test]
fn duplicate_thread_ids_in_recovery_manifest_fail_closed() {
    let test_home = TestCodexHome::new("duplicate-manifest");
    let home = test_home.path().to_path_buf();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let first = PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    };
    let second = PendingTarget {
        awaiting_owner: true,
        owner_account_id: Some("account-a".into()),
        ..first.clone()
    };
    assert!(write_manifest(&[first.clone(), second.clone()]).is_err());
    let bytes = serde_json::to_vec(&super::pending_manifest::PendingManifest {
        version: 1,
        targets: vec![first, second],
    })
    .unwrap();
    std::fs::write(home.join("desktop-recovery.json"), bytes).unwrap();
    assert!(load_manifest().is_err());
    assert!(mark_dispatch_attempt_for_account(
        id,
        Some("account-a"),
        RecoveryMode::DeferredCaptured
    )
    .is_err());
}

#[test]
fn load_pending_filters_stale_targets_without_writing_the_manifest() {
    let test_home = TestCodexHome::new("manifest-prune-test");
    let temp_dir = test_home.path().to_path_buf();

    let stale_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e401";
    let completed_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e402";
    let active_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e403";
    let queued_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e404";

    let now = chrono::Utc::now().timestamp();
    let old_time = now - 4 * 86400; // 4 days ago
    let fresh_time = now - 300; // 5 minutes ago

    let database = temp_dir.join("state_5.sqlite");
    let sql = format!(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT, updated_at INTEGER, rollout_path TEXT);\
         INSERT INTO threads VALUES ('{stale_id}', 0, 'user', {old_time}, '');\
         INSERT INTO threads VALUES ('{completed_id}', 0, 'user', {fresh_time}, '');\
         INSERT INTO threads VALUES ('{active_id}', 0, 'user', {fresh_time}, '');\
         INSERT INTO threads VALUES ('{queued_id}', 0, 'user', {fresh_time}, '');"
    );
    let result = Command::new("/usr/bin/sqlite3")
        .arg(&database)
        .arg(sql)
        .status()
        .unwrap();
    assert!(result.success());

    let sessions = temp_dir.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();

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
    create_rollout(
        active_id,
        r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
    );
    create_rollout(
        queued_id,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t2","error":null}}"#,
    );
    let queue = temp_dir.join("queue_1.sqlite");
    let queue_sql = format!(
        "CREATE TABLE queued_items (thread_id TEXT, payload TEXT); INSERT INTO queued_items VALUES ('{queued_id}', 'restart-paused');"
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(queue_sql)
        .status()
        .unwrap()
        .success());

    // Pre-populate manifest with all three targets
    let initial_targets = vec![
        PendingTarget {
            id: stale_id.to_string(),
            offset: Some(10),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: completed_id.to_string(),
            offset: Some(20),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: active_id.to_string(),
            offset: Some(30),
            awaiting_owner: false,
            captured_restart: false,
            owner_account_id: None,
        },
        PendingTarget {
            id: queued_id.to_string(),
            offset: Some(40),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("account-a".into()),
        },
    ];
    assert!(write_manifest(&initial_targets).is_ok());

    // Calling load_pending must prune stale and completed targets AND persist the cleaned list to disk
    let pending = load_pending().unwrap();
    assert_eq!(pending, vec![active_id.to_string()]);

    // The completed turn stays eligible for deferred queued follow-up
    // recovery, even though load_pending excludes ownerless entries.
    let mut eligible = load_manifest().unwrap();
    super::manifest_store::prune_ineligible_targets(&temp_dir, &mut eligible).unwrap();
    assert_eq!(
        eligible
            .iter()
            .map(|target| target.id.as_str())
            .collect::<Vec<_>>(),
        [active_id, queued_id]
    );

    // The detection read is deliberately non-mutating; the recovery owner
    // prunes stale entries under the operation lock.
    let reloaded = load_manifest().unwrap();
    assert_eq!(reloaded.len(), 4);

    std::fs::write(&queue, b"not a sqlite database").unwrap();
    assert!(load_pending().is_err());
    assert_eq!(load_manifest().unwrap().len(), 4);

    // If active_id completes and targets becomes empty, write_manifest removes the file completely
    assert!(write_manifest(&[]).is_ok());
    assert!(!temp_dir.join("desktop-recovery.json").exists());
    assert!(load_pending().unwrap().is_empty());
}

#[test]
fn fresh_confirmation_rejects_growth_after_scan() {
    use std::io::Write;
    let home = std::env::temp_dir().join(format!("codex-scan-race-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\"}}\n",
    )
    .unwrap();
    let target = PendingTarget {
        id: id.into(),
        offset: Some(11),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("old-account".into()),
    };
    let result = post_checkpoint_status_fresh_after_scan(&home, &target, |path| {
        std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(b"later\n")
            .unwrap();
    });
    assert!(
        result.is_err(),
        "a growing snapshot cannot confirm stale evidence"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn unattended_retry_intent_is_dropped_for_error_ended_turns_only() {
    let home = std::env::temp_dir().join(format!(
        "codex-error-retention-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let error_id = "01a098c2-0fae-74d2-a80c-45d89e910e81";
    let quota_id = "01a098c2-0fae-74d2-a80c-45d89e910e82";
    for (id, line) in [
        (
            error_id,
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":null,"error":{"message":"unexpected status 401 Unauthorized","codex_error_info":"other"}}}"#,
        ),
        (
            quota_id,
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":null,"error":{"message":"out of credits","codex_error_info":"usage_limit_exceeded"}}}"#,
        ),
    ] {
        std::fs::write(
            sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl")),
            format!("{line}\n"),
        )
        .unwrap();
    }
    let target = |id: &str| PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    let mut pending = vec![target(error_id), target(quota_id)];
    let now = chrono::Utc::now().timestamp();
    prune_ineligible_targets_with(&home, &mut pending, |_| Ok(Some(now))).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
    // A deferred worker could never dispatch the error-ended turn, so keeping
    // it would only re-probe it every 15 seconds for four hours.
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, quota_id);
}
