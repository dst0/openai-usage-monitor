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
    for unknown in [
        b"codex://threads/01a00000-0000-4000-8000-00000000000f\n".as_slice(),
        b"WINDOW_FOCUS_FAILED codex://threads/x\n",
        b"WINDOW_FOCUS_FAILED\n\n",
        b"window_focus_failed\n",
        b"",
    ] {
        assert_eq!(WindowTaskProbeValidationService::failure(unknown), None);
    }
}

/// Every failure the probe itself can print must be nameable, and the list
/// must not name a code the helper can no longer print.
#[test]
fn failure_codes_match_the_native_helper() {
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts");
    let codes = |name: &str| -> BTreeSet<String> {
        let source = std::fs::read_to_string(scripts.join(name)).unwrap();
        source
            .split("fail(\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap().to_string())
            .collect()
    };
    let probe = codes("CodexWindowTaskProbe.swift");
    let helper = codes("codex-window-restore.swift");
    let known: BTreeSet<String> = FAILURE_CODES.iter().map(|code| code.to_string()).collect();
    assert!(probe.len() >= 10, "probe failure scan found {probe:?}");
    assert!(
        probe.is_subset(&known),
        "unnamed: {:?}",
        probe.difference(&known)
    );
    let emitted: BTreeSet<String> = probe.union(&helper).cloned().collect();
    assert!(
        known.is_subset(&emitted),
        "stale: {:?}",
        known.difference(&emitted)
    );
}
