use super::{
    dispatch_identity_checks::DispatchIdentityChecks,
    ipc_call_error::IpcCallError,
    manifest_store::finalize_target,
    observer::Observer,
    pending_target::PendingTarget,
    recovery_mode::RecoveryMode,
    recovery_service::mark_dispatch_failure,
    recovery_target::RecoveryTarget,
    target_dispatch::{
        dispatch_if_needed, handle_owner_resolution, revalidate_after_banner_gate,
        revalidate_after_owner, revalidate_after_owner_with_budget,
    },
    test_desktop_router::TestDesktopRouter,
};
use crate::{
    distribution::WindowProcessIdentity, recovery::RecoveryBanner, switcher::ThreadRolloutState,
};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn explicit_owner_wait_refuses_dispatch_after_shared_auth_changes() {
    assert_owner_wait_rejects_account_change(false, false);
}

#[test]
fn queued_owner_wait_refuses_dispatch_after_shared_auth_changes() {
    assert_owner_wait_rejects_account_change(true, false);
}

#[test]
fn explicit_marker_write_refuses_dispatch_after_shared_auth_changes() {
    assert_owner_wait_rejects_account_change(false, true);
}

#[test]
fn queued_marker_write_refuses_dispatch_after_shared_auth_changes() {
    assert_owner_wait_rejects_account_change(true, true);
}

fn assert_owner_wait_rejects_account_change(with_queue: bool, change_after_marker: bool) {
    let env = crate::distribution::test_helper::TestEnv::new("owner_wait_auth_change");
    let first = crate::distribution::test_account_spec::TestAccountSpec {
        id: "account-a",
        email: "first@example.test",
        plan: "plus",
        sprint_pct: 20.0,
        ..crate::distribution::test_account_spec::TestAccountSpec::default()
    }
    .build();
    let second = crate::distribution::test_account_spec::TestAccountSpec {
        id: "account-b",
        email: "second@example.test",
        plan: "plus",
        sprint_pct: 20.0,
        ..crate::distribution::test_account_spec::TestAccountSpec::default()
    }
    .build();
    env.populate(vec![first, second], Some("account-a"), Some("account-a"));
    let accounts = crate::storage::load_accounts().unwrap();
    let start_account = super::manifest_store::current_account_binding().unwrap();
    let next_account = accounts
        .accounts
        .iter()
        .find(|account| account.id != start_account)
        .unwrap();
    let mut next_auth = crate::storage::read_active_auth_json().unwrap();
    next_auth.tokens = Some(next_account.tokens.clone());
    let next_account_id = next_account.id.clone();

    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(env.home(), id);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":{\"code\":\"usage_limit_exceeded\"}}}\n")
        .unwrap();
    candidate.observer = Observer::checkpoint(candidate.observer.path.clone()).unwrap();
    candidate.state = ThreadRolloutState::InterruptedByQuota;
    make_rollout_discoverable(env.home(), &mut candidate);
    let original = PendingTarget {
        id: id.into(),
        offset: Some(candidate.observer.offset),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some(start_account.clone()),
    };
    super::manifest_store::write_manifest(std::slice::from_ref(&original)).unwrap();
    if with_queue {
        let queue = env.home().join("queue_1.sqlite");
        let sql = format!(
            r#"CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER);
               CREATE TABLE queued_items (thread_id TEXT, queue_order INTEGER, payload_json TEXT);
               INSERT INTO queued_items VALUES ('{id}', 1, '{{"id":"queued-one"}}');"#
        );
        assert!(Command::new("/usr/bin/sqlite3")
            .arg(&queue)
            .arg(sql)
            .status()
            .unwrap()
            .success());
    }
    let router_auth = next_auth.clone();
    // Owner discovery is the only expected request. Any later request is
    // recorded rather than lost to a router timeout, so a dispatch that
    // crosses the account change fails this test however slowly it happens.
    let (mut desktop, router) = TestDesktopRouter::start(move |request| {
        if request["method"] != "thread-owner-discovery" {
            return Some(TestDesktopRouter::success(
                request,
                "window-one",
                serde_json::json!({"ok": true, "result": {"turn": {"id": id}}}),
            ));
        }
        if !change_after_marker {
            crate::storage::write_active_auth_json(&router_auth).unwrap();
        }
        Some(TestDesktopRouter::success(
            request,
            "window-one",
            serde_json::json!({ "supportsUntrustedAppInput": true }),
        ))
    });
    let mut budget = super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES;
    let identity_checks = std::cell::Cell::new(0);
    let result = dispatch_if_needed(
        env.home(),
        &mut desktop,
        &mut candidate,
        RecoveryMode::ExplicitTarget,
        &mut budget,
        || Ok(()),
        &mut DispatchIdentityChecks::new(
            || {
                let matched = super::manifest_store::current_account_binding().as_deref()
                    == Some(start_account.as_str());
                if matched && change_after_marker && identity_checks.get() == 0 {
                    identity_checks.set(1);
                    crate::storage::write_active_auth_json(&next_auth).unwrap();
                }
                matched
                    .then_some(())
                    .ok_or(super::dispatch_mark_error::DispatchMarkError::AccountChanged)
            },
            // The explicit request claimed this deferred target, so its
            // Desktop binding is irrelevant and must not be resolved: in
            // production that runs the installed helper on live ChatGPT.
            unconsulted_deferred_binding,
        ),
    );
    let requests = router.finish(desktop);
    let methods = request_methods(&requests);
    let observed_account = super::manifest_store::current_account_binding();
    let retained = super::manifest_store::load_manifest().unwrap();
    let safe = observed_account.as_deref() == Some(next_account_id.as_str())
        && result.is_err()
        && methods == ["thread-owner-discovery"]
        && !candidate.dispatched
        && retained == vec![original];
    drop(env);
    assert!(
        safe,
        "owner wait crossed an account change: result={result:?}, methods={methods:?}, dispatched={}, retained={retained:?}, observed_account={observed_account:?}",
        candidate.dispatched,
    );
}

#[test]
fn already_unpaused_queue_sends_one_owner_routed_wake_before_consuming_checkpoint() {
    assert_unpaused_queue_wake(Duration::ZERO);
}

#[test]
fn unpaused_queue_wake_survives_a_slow_banner_gate() {
    assert_unpaused_queue_wake(Duration::from_millis(750));
}

fn assert_unpaused_queue_wake(gate_delay: Duration) {
    let env = crate::distribution::test_helper::TestEnv::new("queue_owner_wake");
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(env.home(), id);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":{\"code\":\"usage_limit_exceeded\"}}}\n")
        .unwrap();
    candidate.observer = Observer::checkpoint(candidate.observer.path.clone()).unwrap();
    candidate.state = ThreadRolloutState::InterruptedByQuota;
    make_rollout_discoverable(env.home(), &mut candidate);
    let queue = env.home().join("queue_1.sqlite");
    let sql = format!(
        r#"CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER);
           CREATE TABLE queued_items (thread_id TEXT, queue_order INTEGER, payload_json TEXT);
           INSERT INTO queued_items VALUES ('{id}', 1, '{{"id":"queued-one","context":{{"keep":true}}}}');"#
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    super::manifest_store::write_manifest(&[PendingTarget {
        id: id.into(),
        offset: Some(candidate.observer.offset),
        awaiting_owner: false,
        captured_restart: false,
        owner_account_id: None,
    }])
    .unwrap();
    // The router stays connected until the client hangs up, so the banner
    // gate may take as long as it does in production (panel startup can take
    // seconds) without the router giving up and turning the wake into EPIPE.
    let (mut desktop, router) = TestDesktopRouter::start(|request| {
        let result = if request["method"] == "thread-owner-discovery" {
            serde_json::json!({ "supportsUntrustedAppInput": true })
        } else {
            serde_json::json!({ "ok": true })
        };
        Some(TestDesktopRouter::success(request, "window-one", result))
    });
    let mut budget = super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES;
    let mut gate_delay = Some(gate_delay);
    let result = dispatch_if_needed(
        env.home(),
        &mut desktop,
        &mut candidate,
        RecoveryMode::DiscoveredOnly,
        &mut budget,
        || {
            // Only the first banner check is slow; it sits between owner
            // discovery and the wake, where the old 500 ms router gave up.
            thread::sleep(gate_delay.take().unwrap_or_default());
            Ok(())
        },
        &mut DispatchIdentityChecks::new(|| Ok(()), unconsulted_deferred_binding),
    );
    let requests = router.finish(desktop);
    let methods = request_methods(&requests);
    let routed = requests.get(1).is_some_and(|wake| {
        wake["targetClientId"] == "window-one"
            && wake["params"]["state"][id][0]
                == serde_json::json!({"id":"queued-one","context":{"keep":true}})
    });
    let consumed = super::manifest_store::load_manifest().unwrap().is_empty();
    let passed = result.is_ok()
        && methods
            == [
                "thread-owner-discovery",
                "thread-follower-set-queued-follow-ups-state",
            ]
        && routed
        && candidate.dispatched
        && consumed;
    drop(env);
    assert!(
        passed,
        "unpaused queue did not complete one owner-routed IPC wake: result_ok={}, error={:?}, methods={methods:?}, routed={routed}, dispatched={}, consumed={consumed}",
        result.is_ok(),
        result.as_ref().err(),
        candidate.dispatched
    );
}

#[test]
fn deferred_dispatch_refuses_a_checkpoint_bound_to_another_desktop_account() {
    for binding in [Some("account-b"), None] {
        let outcome = dispatch_deferred_target_under(binding);
        assert!(
            outcome.error.as_deref()
                == Some("Active Desktop account or process changed before recovery dispatch")
                && outcome.account_mismatch
                && !outcome.dispatched
                && outcome.checkpoint_retained
                && outcome.methods == ["thread-owner-discovery"]
                && outcome.binding_reads == 1
                && outcome.identity_checks == 0,
            "binding={binding:?}: {outcome:?}"
        );
    }
}

#[test]
fn deferred_dispatch_consumes_the_checkpoint_for_its_bound_desktop_account() {
    let outcome = dispatch_deferred_target_under(Some("account-a"));
    assert!(
        outcome.error.is_none()
            && !outcome.account_mismatch
            && outcome.dispatched
            && !outcome.checkpoint_retained
            && outcome.methods == ["thread-owner-discovery", "thread-follower-start-turn"]
            && outcome.binding_reads == 1
            && outcome.identity_checks == 2,
        "{outcome:?}"
    );
}

#[derive(Debug)]
struct DeferredDispatchOutcome {
    error: Option<String>,
    account_mismatch: bool,
    dispatched: bool,
    checkpoint_retained: bool,
    methods: Vec<String>,
    binding_reads: usize,
    identity_checks: usize,
}

/// Dispatches a quota-interrupted target that `account-a` deferred, while the
/// injected Desktop session resolves to `binding`.
fn dispatch_deferred_target_under(binding: Option<&str>) -> DeferredDispatchOutcome {
    let env = crate::distribution::test_helper::TestEnv::new("deferred_dispatch_binding");
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(env.home(), id);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":{\"code\":\"usage_limit_exceeded\"}}}\n")
        .unwrap();
    candidate.observer = Observer::checkpoint(candidate.observer.path.clone()).unwrap();
    candidate.state = ThreadRolloutState::InterruptedByQuota;
    make_rollout_discoverable(env.home(), &mut candidate);
    let original = PendingTarget {
        id: id.into(),
        offset: Some(candidate.observer.offset),
        awaiting_owner: true,
        captured_restart: true,
        owner_account_id: Some("account-a".into()),
    };
    super::manifest_store::write_manifest(std::slice::from_ref(&original)).unwrap();
    let (mut desktop, router) = TestDesktopRouter::start(move |request| {
        let result = if request["method"] == "thread-owner-discovery" {
            serde_json::json!({ "supportsUntrustedAppInput": true })
        } else {
            serde_json::json!({ "result": { "turn": { "id": id } } })
        };
        Some(TestDesktopRouter::success(request, "window-one", result))
    });
    let (binding_reads, identity_checks) = (std::cell::Cell::new(0), std::cell::Cell::new(0));
    let mut budget = super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES;
    let result = dispatch_if_needed(
        env.home(),
        &mut desktop,
        &mut candidate,
        RecoveryMode::DeferredCaptured,
        &mut budget,
        || Ok(()),
        &mut DispatchIdentityChecks::new(
            || {
                identity_checks.set(identity_checks.get() + 1);
                Ok(())
            },
            || {
                binding_reads.set(binding_reads.get() + 1);
                binding.map(str::to_owned)
            },
        ),
    );
    let methods = request_methods(&router.finish(desktop))
        .into_iter()
        .map(str::to_owned)
        .collect();
    let checkpoint_retained = super::manifest_store::load_manifest().unwrap() == vec![original];
    drop(env);
    DeferredDispatchOutcome {
        error: result.err(),
        account_mismatch: candidate.account_mismatch,
        dispatched: candidate.dispatched,
        checkpoint_retained,
        methods,
        binding_reads: binding_reads.get(),
        identity_checks: identity_checks.get(),
    }
}

fn unconsulted_deferred_binding() -> Option<String> {
    panic!("this dispatch must not resolve a deferred Desktop account binding")
}

fn request_methods(requests: &[serde_json::Value]) -> Vec<&str> {
    requests
        .iter()
        .map(|request| request["method"].as_str().unwrap_or("<missing>"))
        .collect()
}

#[test]
fn discovery_only_queued_user_stop_keeps_checkpoint_without_owner_ipc() {
    assert_ineligible_queued_turn_never_contacts_owner(
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\"}}\n",
        ThreadRolloutState::TurnAborted,
        RecoveryMode::DiscoveredOnly,
    );
}

#[test]
fn captured_queued_nonquota_error_keeps_checkpoint_without_owner_ipc() {
    assert_ineligible_queued_turn_never_contacts_owner(
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":{\"code\":\"synthetic_policy_error\"}}}\n",
        ThreadRolloutState::InterruptedByError,
        RecoveryMode::CapturedRestart,
    );
}

fn assert_ineligible_queued_turn_never_contacts_owner(
    terminal_event: &[u8],
    state: ThreadRolloutState,
    mode: RecoveryMode,
) {
    let env = crate::distribution::test_helper::TestEnv::new("queue_ineligible_mode");
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(env.home(), id);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(terminal_event)
        .unwrap();
    candidate.observer = Observer::checkpoint(candidate.observer.path.clone()).unwrap();
    candidate.state = state;
    make_rollout_discoverable(env.home(), &mut candidate);
    assert_eq!(
        crate::switcher::inspect_thread_rollout_state(env.home(), id),
        state
    );
    let queue = env.home().join("queue_1.sqlite");
    let sql = format!(
        r#"CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER);
           CREATE TABLE queued_items (thread_id TEXT, queue_order INTEGER, payload_json TEXT);
           INSERT INTO queued_items VALUES ('{id}', 1, '{{"id":"queued-one"}}');"#
    );
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        super::queue_snapshot::queued_messages(env.home(), id)
            .unwrap()
            .len(),
        1
    );
    super::manifest_store::write_manifest(&[PendingTarget {
        id: id.into(),
        offset: Some(candidate.observer.offset),
        awaiting_owner: false,
        captured_restart: mode == RecoveryMode::CapturedRestart,
        owner_account_id: None,
    }])
    .unwrap();
    // No request is expected; the client's hang-up, not a timeout, ends the
    // router, so even a late owner request would be recorded.
    let (mut desktop, router) = TestDesktopRouter::start(|request| {
        Some(TestDesktopRouter::success(
            request,
            "window-one",
            serde_json::json!({ "supportsUntrustedAppInput": true, "ok": true }),
        ))
    });
    let mut budget = super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES;
    let result = dispatch_if_needed(
        env.home(),
        &mut desktop,
        &mut candidate,
        mode,
        &mut budget,
        || Ok(()),
        &mut DispatchIdentityChecks::new(|| Ok(()), unconsulted_deferred_binding),
    );
    let requests = router.finish(desktop);
    let methods = request_methods(&requests);
    let saved = super::manifest_store::load_manifest().unwrap();
    let retained =
        saved.len() == 1 && saved[0].id == id && saved[0].offset == Some(candidate.observer.offset);
    let rejected =
        matches!(&result, Err(error) if error.contains("not eligible for queued recovery"));
    let dispatched = candidate.dispatched;
    drop(env);
    assert!(
        rejected && methods.is_empty() && retained && !dispatched,
        "ineligible queued recovery touched owner/checkpoint: rejected={rejected}, methods={methods:?}, retained={retained}, dispatched={dispatched}, error={:?}",
        result.err()
    );
}

#[test]
fn queued_user_abort_requires_a_captured_or_explicit_target() {
    let home = std::env::temp_dir().join(format!(
        "codex-queued-abort-mode-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\"}}\n")
        .unwrap();
    candidate.observer = Observer::checkpoint(candidate.observer.path.clone()).unwrap();
    candidate.state = ThreadRolloutState::TurnAborted;
    make_rollout_discoverable(&home, &mut candidate);
    let queue = home.join("queue_1.sqlite");
    let sql = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    let discovered =
        revalidate_after_owner(&home, &mut candidate, 0, 1, RecoveryMode::DiscoveredOnly);
    let captured =
        revalidate_after_owner(&home, &mut candidate, 0, 1, RecoveryMode::CapturedRestart);
    std::fs::remove_dir_all(home).unwrap();
    assert!(
        discovered.is_err() && captured == Ok(true),
        "queued user Stop bypassed mode gating"
    );
}

#[test]
fn queued_nonquota_error_requires_an_explicit_target() {
    let home = std::env::temp_dir().join(format!(
        "codex-queued-error-mode-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":{\"code\":\"synthetic_policy_error\"}}}\n")
        .unwrap();
    candidate.observer = Observer::checkpoint(candidate.observer.path.clone()).unwrap();
    candidate.state = ThreadRolloutState::InterruptedByError;
    make_rollout_discoverable(&home, &mut candidate);
    let queue = home.join("queue_1.sqlite");
    let sql = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    let discovered =
        revalidate_after_owner(&home, &mut candidate, 0, 1, RecoveryMode::DiscoveredOnly);
    let explicit =
        revalidate_after_owner(&home, &mut candidate, 0, 1, RecoveryMode::ExplicitTarget);
    std::fs::remove_dir_all(home).unwrap();
    assert!(
        discovered.is_err() && explicit == Ok(true),
        "queued nonquota error bypassed explicit-only recovery"
    );
}

fn target(home: &Path, id: &str) -> RecoveryTarget {
    let rollout = home.join(format!("rollout-{id}.jsonl"));
    std::fs::write(
        &rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}\n",
    )
    .unwrap();
    RecoveryTarget {
        id: id.into(),
        state: ThreadRolloutState::ActiveInProgress,
        writer_locked: false,
        observer: Observer::checkpoint(rollout).unwrap(),
        scan_complete: true,
        existing_queue: 0,
        mounted_by_recovery: false,
        owner_unavailable: false,
        account_mismatch: false,
        dispatched: false,
        completed: false,
        failure: None,
        deadline: Instant::now(),
        execution_deadline_set: false,
        expected_turn_id: None,
        proof_observed_at: None,
    }
}

fn make_rollout_discoverable(home: &Path, target: &mut RecoveryTarget) {
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-{}.jsonl", target.id));
    std::fs::rename(&target.observer.path, &rollout).unwrap();
    target.observer = Observer::checkpoint(rollout).unwrap();
}

#[test]
fn absent_panel_after_owner_keeps_original_checkpoint_for_deferred_retry() {
    let home = std::env::temp_dir().join(format!("codex-late-banner-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let mut banner = RecoveryBanner::without_window(process.clone());
    let error = banner
        .ensure_visible_after_owner_with(false, || Ok(RecoveryBanner::without_window(process)))
        .unwrap_err();
    let mut candidate = target(&home, id);
    mark_dispatch_failure(&mut candidate, &error);
    assert!(candidate.owner_unavailable);
    assert!(!candidate.dispatched);
    let mut manifest = vec![PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    }];
    finalize_target(
        &mut manifest,
        id,
        candidate.owner_unavailable,
        candidate.dispatched,
        Some("account-a"),
        false,
    );
    assert_eq!(manifest[0].offset, Some(42));
    assert!(manifest[0].awaiting_owner);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_revalidation_rejects_an_unscanned_rollout_backlog() {
    let home = std::env::temp_dir().join(format!("codex-owner-backlog-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let mut writer = OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap();
    let record = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
        "x".repeat(8 * 1024)
    );
    for _ in 0..(32 * 1024 * 1024_usize).div_ceil(record.len()) {
        writer.write_all(record.as_bytes()).unwrap();
    }
    writer.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n").unwrap();
    drop(writer);
    let result =
        revalidate_after_owner(&home, &mut candidate, 0, 0, RecoveryMode::DeferredCaptured);
    assert!(result.unwrap_err().contains("still being checked"));
    assert!(!candidate.scan_complete);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn oversized_completed_start_cannot_authorize_foreground_dispatch() {
    let home = std::env::temp_dir().join(format!("codex-oversized-start-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut candidate = target(&home, "01a098c2-0fae-74d2-a80c-45d89e910e79");
    let event = serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"user-turn","padding":"x".repeat(super::observer::MAX_LINE)}});
    let mut writer = OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap();
    writer.write_all(event.to_string().as_bytes()).unwrap();
    writer.write_all(b"\n").unwrap();
    drop(writer);
    let result = super::recovery_target::record_target_state_at(&mut candidate, Instant::now());
    assert!(result.is_err() || !candidate.scan_complete);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn oversized_completed_error_cannot_verify_recovered_turn() {
    let home = std::env::temp_dir().join(format!("codex-oversized-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut candidate = target(&home, "01a098c2-0fae-74d2-a80c-45d89e910e79");
    candidate.dispatched = true;
    candidate.expected_turn_id = Some("recovery-turn".into());
    let started = serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"recovery-turn"}});
    let work = serde_json::json!({"type":"response_item","payload":{"type":"reasoning"}});
    let failure = serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"recovery-turn","error":{"codex_error_info":"usage_limit_exceeded","padding":"x".repeat(super::observer::MAX_LINE)}}});
    let mut writer = OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap();
    for event in [started, work, failure] {
        writer.write_all(event.to_string().as_bytes()).unwrap();
        writer.write_all(b"\n").unwrap();
    }
    drop(writer);
    let now = Instant::now();
    let _ = super::recovery_target::record_target_state_at(&mut candidate, now);
    let result = super::recovery_target::record_target_state_at(
        &mut candidate,
        now + super::recovery_target::RECOVERY_SOAK_WINDOW,
    );
    assert!(result.is_err() || !candidate.completed);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_revalidation_bounds_scans_across_two_targets() {
    let home = std::env::temp_dir().join(format!("codex-owner-fair-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut candidates = [
        target(&home, "01a098c2-0fae-74d2-a80c-45d89e910e71"),
        target(&home, "01a098c2-0fae-74d2-a80c-45d89e910e72"),
    ];
    let initial_offsets = candidates
        .iter()
        .map(|candidate| candidate.observer.offset)
        .collect::<Vec<_>>();
    for candidate in &candidates {
        let mut writer = OpenOptions::new()
            .append(true)
            .open(&candidate.observer.path)
            .unwrap();
        let record = format!(
            "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"padding\":\"{}\"}}}}\n",
            "x".repeat(8 * 1024)
        );
        for _ in 0..(32 * 1024 * 1024_usize).div_ceil(record.len()) {
            writer.write_all(record.as_bytes()).unwrap();
        }
    }
    let per_target = super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES / candidates.len() as u64;
    for candidate in &mut candidates {
        let mut budget = per_target;
        assert!(revalidate_after_owner_with_budget(
            &home,
            candidate,
            0,
            0,
            RecoveryMode::DeferredCaptured,
            &mut budget,
        )
        .unwrap_err()
        .contains("still being checked"));
        assert_eq!(budget, 0);
    }
    let scanned = candidates
        .iter()
        .zip(initial_offsets)
        .map(|(candidate, initial)| candidate.observer.offset - initial)
        .sum::<u64>();
    assert!(scanned <= super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES);
    assert!(candidates.iter().all(|candidate| !candidate.dispatched));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn turn_started_while_banner_appears_prevents_old_snapshot_dispatch() {
    let home = std::env::temp_dir().join(format!("codex-banner-race-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let rollout = candidate.observer.path.clone();
    let ready = revalidate_after_banner_gate(
        &home,
        &mut candidate,
        0,
        0,
        RecoveryMode::DeferredCaptured,
        || {
            OpenOptions::new()
                .append(true)
                .open(&rollout)
                .unwrap()
                .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n")
                .unwrap();
            Ok(())
        },
    )
    .unwrap();
    assert!(!ready);
    assert!(candidate.observer.evidence.started);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn queue_changed_while_banner_appears_prevents_old_snapshot_dispatch() {
    let home = std::env::temp_dir().join(format!("codex-banner-queue-race-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let queue = home.join("queue_1.sqlite");
    let sql = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    let result = revalidate_after_banner_gate(
        &home,
        &mut candidate,
        0,
        0,
        RecoveryMode::DeferredCaptured,
        || {
            assert!(Command::new("/usr/bin/sqlite3")
                .arg(&queue)
                .arg(&sql)
                .status()
                .unwrap()
                .success());
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn panel_exit_after_sqlite_recheck_still_prevents_dispatch() {
    let home = std::env::temp_dir().join(format!("codex-banner-final-gate-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    make_rollout_discoverable(&home, &mut candidate);
    let queue = home.join("queue_1.sqlite");
    let setup = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(setup)
        .status()
        .unwrap()
        .success());
    let mut checks = 0;
    let result = revalidate_after_banner_gate(
        &home,
        &mut candidate,
        0,
        1,
        RecoveryMode::DeferredCaptured,
        || {
            checks += 1;
            if checks == 2 {
                return Err("Recovery banner helper exited before IPC dispatch".into());
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(checks, 2);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn turn_started_during_final_panel_check_prevents_stale_dispatch() {
    let home = std::env::temp_dir().join(format!(
        "codex-banner-final-turn-race-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    make_rollout_discoverable(&home, &mut candidate);
    let rollout = candidate.observer.path.clone();
    let queue = home.join("queue_1.sqlite");
    let setup = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(setup)
        .status()
        .unwrap()
        .success());
    let mut checks = 0;
    let ready = revalidate_after_banner_gate(
        &home,
        &mut candidate,
        0,
        1,
        RecoveryMode::DeferredCaptured,
        || {
            checks += 1;
            if checks == 2 {
                OpenOptions::new()
                    .append(true)
                    .open(&rollout)
                    .unwrap()
                    .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n")
                    .unwrap();
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(checks, 2);
    assert!(!ready);
    assert!(candidate.observer.evidence.started);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn queued_payload_revision_change_during_banner_gate_blocks_dispatch() {
    let home = std::env::temp_dir().join(format!(
        "codex-banner-queue-revision-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let queue = home.join("queue_1.sqlite");
    let setup = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}'); INSERT INTO queued_thread_revisions VALUES ('{id}', 1);");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(setup)
        .status()
        .unwrap()
        .success());
    let result = revalidate_after_banner_gate(
        &home,
        &mut candidate,
        1,
        1,
        RecoveryMode::DeferredCaptured,
        || {
            assert!(Command::new("/usr/bin/sqlite3")
                .arg(&queue)
                .arg(format!(
                    "UPDATE queued_thread_revisions SET revision=2 WHERE thread_id='{id}';"
                ))
                .status()
                .unwrap()
                .success());
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_navigation_before_dispatch_keeps_original_checkpoint() {
    let home = std::env::temp_dir().join(format!("codex-navigation-error-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    assert!(handle_owner_resolution(
        Err(IpcCallError::Other("LaunchServices unavailable".into())),
        &mut candidate,
    )
    .is_err());
    assert!(candidate.owner_unavailable);
    assert!(!candidate.dispatched);
    let mut manifest = vec![PendingTarget {
        id: id.into(),
        offset: Some(42),
        awaiting_owner: false,
        captured_restart: true,
        owner_account_id: None,
    }];
    finalize_target(
        &mut manifest,
        id,
        candidate.owner_unavailable,
        candidate.dispatched,
        Some("account-a"),
        false,
    );
    assert_eq!(manifest[0].offset, Some(42));
    assert!(manifest[0].awaiting_owner);
    assert_eq!(manifest[0].owner_account_id.as_deref(), Some("account-a"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_wait_recheck_does_not_dispatch_after_a_new_turn_starts() {
    let home = std::env::temp_dir().join(format!("codex-owner-turn-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let mut file = OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap();
    file.write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n").unwrap();
    assert!(
        !revalidate_after_owner(&home, &mut candidate, 0, 0, RecoveryMode::DeferredCaptured)
            .unwrap()
    );
    assert!(candidate.observer.evidence.started);
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_revalidation_rejects_middle_rewrite_followed_by_append() {
    use std::io::{Seek, SeekFrom};
    let home = std::env::temp_dir().join(format!("codex-owner-rewrite-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut candidate = target(&home, id);
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::rename(&candidate.observer.path, &rollout).unwrap();
    candidate.observer = Observer::checkpoint(rollout).unwrap();
    let checkpoint = candidate.observer.offset;
    let started = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"user-turn\"}}\n";
    let mut metadata = b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"}}".to_vec();
    metadata.resize(started.len() - 1, b' ');
    metadata.push(b'\n');
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(&metadata)
        .unwrap();
    candidate.observer =
        Observer::checkpoint_at(candidate.observer.path.clone(), checkpoint).unwrap();
    super::recovery_target::record_target_state_with_budget(&mut candidate, 1024).unwrap();
    assert!(candidate.scan_complete);
    assert!(!candidate.observer.evidence.started);

    let mut file = OpenOptions::new()
        .write(true)
        .append(false)
        .open(&candidate.observer.path)
        .unwrap();
    file.seek(SeekFrom::Start(checkpoint)).unwrap();
    file.write_all(started).unwrap();
    drop(file);
    OpenOptions::new()
        .append(true)
        .open(&candidate.observer.path)
        .unwrap()
        .write_all(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\"}}\n")
        .unwrap();

    let error = revalidate_after_owner(&home, &mut candidate, 0, 0, RecoveryMode::DeferredCaptured)
        .unwrap_err();
    assert!(error.contains("Rollout identity changed"), "{error}");
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn owner_wait_recheck_rejects_a_new_queued_follow_up() {
    let home = std::env::temp_dir().join(format!("codex-owner-queue-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e80";
    let mut candidate = target(&home, id);
    let queue = home.join("queue_1.sqlite");
    let sql = format!("CREATE TABLE queued_thread_revisions (thread_id TEXT, revision INTEGER); CREATE TABLE queued_items (thread_id TEXT); INSERT INTO queued_items VALUES ('{id}');");
    assert!(Command::new("/usr/bin/sqlite3")
        .arg(&queue)
        .arg(sql)
        .status()
        .unwrap()
        .success());
    assert!(
        revalidate_after_owner(&home, &mut candidate, 0, 0, RecoveryMode::DeferredCaptured)
            .is_err()
    );
    assert!(!candidate.dispatched);
    std::fs::remove_dir_all(home).unwrap();
}
