use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn expected() -> ProcessIdentity {
    ProcessIdentity::new(4242, "1726789012:000007").unwrap()
}

fn response(window_ids: serde_json::Value, count: u64) -> serde_json::Value {
    json!({
        "process": {"pid": 4242, "birth_id": "1726789012:000007"},
        "window_ids": window_ids,
        "observed_task_count": count
    })
}

#[test]
fn accepts_only_a_complete_count_for_the_exact_process() {
    let valid = response(json!([31, 32]), 2);
    assert_eq!(
        WindowTaskProbeValidationService::parse(&valid, &expected()),
        Ok(2)
    );
    let mut changed = valid.clone();
    changed["process"]["birth_id"] = json!("1726789012:000008");
    assert!(WindowTaskProbeValidationService::parse(&changed, &expected()).is_err());
    changed = valid.clone();
    changed["process"]["pid"] = json!(4243);
    assert!(WindowTaskProbeValidationService::parse(&changed, &expected()).is_err());
    changed = valid;
    changed["process"]["birth_id"] = json!("1726789012:\n");
    assert!(WindowTaskProbeValidationService::parse(&changed, &expected()).is_err());
}

#[test]
fn every_window_must_yield_one_distinct_task() {
    for (ids, count) in [
        (json!([31, 32]), 1),
        (json!([31, 32]), 3),
        (json!([31, 31]), 2),
        (json!([]), 0),
        (json!([0]), 1),
        (json!([-1]), 1),
        (json!([4_294_967_296_u64]), 1),
        (json!(["31"]), 1),
        (json!([31.5]), 1),
    ] {
        let candidate = response(ids.clone(), count);
        assert!(
            WindowTaskProbeValidationService::parse(&candidate, &expected()).is_err(),
            "accepted {ids} with count {count}"
        );
    }
}

#[test]
fn window_limit_matches_the_native_helper() {
    let at_limit: Vec<u32> = (1..=64).collect();
    let over_limit: Vec<u32> = (1..=65).collect();
    assert_eq!(
        WindowTaskProbeValidationService::parse(&response(json!(at_limit), 64), &expected()),
        Ok(64)
    );
    assert!(
        WindowTaskProbeValidationService::parse(&response(json!(over_limit), 65), &expected())
            .is_err()
    );
}

#[test]
fn response_may_not_carry_extra_or_missing_fields() {
    let mut extra = response(json!([31]), 1);
    extra["task_ids"] = json!(["01a00000-0000-4000-8000-00000000000f"]);
    assert_eq!(
        WindowTaskProbeValidationService::parse(&extra, &expected()),
        Err("Task probe returned unexpected fields".into())
    );
    for field in RESPONSE_FIELDS {
        let mut missing = response(json!([31]), 1);
        missing.as_object_mut().unwrap().remove(field);
        assert!(WindowTaskProbeValidationService::parse(&missing, &expected()).is_err());
    }
    let mut nested = response(json!([31]), 1);
    nested["process"]["task_id"] = json!("01a00000-0000-4000-8000-00000000000f");
    assert_eq!(
        WindowTaskProbeValidationService::parse(&nested, &expected()),
        Err("Task probe returned unexpected fields".into())
    );
    for malformed in [json!(null), json!([]), json!("ok")] {
        assert!(WindowTaskProbeValidationService::parse(&malformed, &expected()).is_err());
    }
}

#[test]
fn only_fixed_failure_codes_are_named() {
    assert_eq!(
        WindowTaskProbeValidationService::failure(b"WINDOW_FOCUS_FAILED\n").as_deref(),
        Some("Task probe failed: WINDOW_FOCUS_FAILED")
    );
    assert_eq!(
        WindowTaskProbeValidationService::failure(b"COMMAND_REJECTED").as_deref(),
        Some("Task probe failed: COMMAND_REJECTED")
    );
    assert_eq!(
        WindowTaskProbeValidationService::failure(b"COPY_LINK_AMBIGUOUS after-focus\n").as_deref(),
        Some(
            "Task probe failed: COPY_LINK_AMBIGUOUS after it began focusing ChatGPT windows; \
             the clipboard may now hold a copied task link"
        )
    );
    for unknown in [
        b"codex://threads/01a00000-0000-4000-8000-00000000000f\n".as_slice(),
        b"WINDOW_FOCUS_FAILED codex://threads/x\n",
        b"WINDOW_FOCUS_FAILED\n\n",
        b"WINDOW_FOCUS_FAILED after-focus after-focus\n",
        b"UNKNOWN_CODE after-focus\n",
        b" after-focus\n",
        b"window_focus_failed\n",
        b"",
    ] {
        assert_eq!(WindowTaskProbeValidationService::failure(unknown), None);
    }
}

/// Codes the probe path reaches through helper functions it shares with the
/// restart guard (`expectedProcess`, `countStandardWindows`).
const SHARED_HELPER_CODES: [&str; 4] = [
    "PROCESS_IDENTITY_REJECTED",
    "WINDOW_ACCESS_FAILED",
    "WINDOW_GEOMETRY_FAILED",
    "WINDOW_INVENTORY_MISMATCH",
];

/// Quoted codes after `marker` on non-comment lines of a helper source.
fn swift_codes(source: &str, marker: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(|line| line.split(marker).skip(1))
        .map(|rest| rest.split('"').next().unwrap().to_string())
        .collect()
}

/// Every failure the probe can print must be nameable, and the list must not
/// name a code the helper can no longer print.
#[test]
fn failure_codes_match_the_native_helper() {
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts");
    let read = |name: &str| std::fs::read_to_string(scripts.join(name)).unwrap();
    let core = read("CodexWindowTaskProbeCore.swift");
    let helper = read("codex-window-restore.swift");
    let mut probe = swift_codes(&core, " = \"");
    probe.extend(swift_codes(&read("CodexWindowTaskProbe.swift"), "fail(\""));
    probe.extend(SHARED_HELPER_CODES.iter().map(|code| code.to_string()));
    let known: BTreeSet<String> = FAILURE_CODES.iter().map(|code| code.to_string()).collect();
    assert!(probe.len() >= 20, "probe failure scan found {probe:?}");
    let unnamed: Vec<_> = probe.difference(&known).collect();
    assert!(unnamed.is_empty(), "unnamed: {unnamed:?}");
    let mut emitted = probe.clone();
    emitted.extend(swift_codes(&helper, "fail(\""));
    let stale: Vec<_> = known.difference(&emitted).collect();
    assert!(stale.is_empty(), "stale: {stale:?}");
    let helper_shared = swift_codes(&helper, "fail(\"");
    assert!(SHARED_HELPER_CODES
        .iter()
        .all(|code| helper_shared.contains(*code)));
    assert!(
        core.contains(&format!(
            "let maximumProbedWindows = {MAX_PROBED_WINDOWS}\n"
        )),
        "the helper's window limit differs from MAX_PROBED_WINDOWS"
    );
}
