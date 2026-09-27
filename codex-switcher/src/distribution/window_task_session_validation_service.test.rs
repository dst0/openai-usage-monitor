use super::*;
use serde_json::json;

const A: &str = "01a00000-0000-4000-8000-00000000000a";
const B: &str = "01a00000-0000-4000-8000-00000000000b";

fn expected() -> ProcessIdentity {
    ProcessIdentity::new(4242, "1726789012:000007").unwrap()
}

fn process() -> Value {
    json!({"pid": 4242, "birth_id": "1726789012:000007"})
}

fn window(id: u64, x: f64, task: &str, focused: bool) -> Value {
    json!({
        "window_id": id,
        "frame": {"x": x, "y": 30.0, "width": 900.0, "height": 700.0},
        "task_id": task,
        "focused": focused,
    })
}

fn snapshot(windows: Vec<Value>) -> Value {
    json!({"process": process(), "windows": windows, "clipboard_restored": true})
}

#[test]
fn a_complete_snapshot_is_parsed_in_capture_order() {
    let parsed = WindowTaskSessionValidationService::snapshot(
        &snapshot(vec![window(52, 0.0, A, false), window(31, 950.0, B, true)]),
        &expected(),
    )
    .unwrap();
    assert_eq!(parsed.process, expected());
    assert_eq!(
        parsed
            .windows
            .iter()
            .map(|entry| (entry.window_id, entry.task_id.as_str(), entry.focused))
            .collect::<Vec<_>>(),
        [(52, A, false), (31, B, true)]
    );
    assert_eq!(parsed.windows[1].frame.x, 950.0);
}

#[test]
fn snapshot_entries_must_be_exact_canonical_and_unique() {
    let invalid = [
        vec![],
        vec![window(52, 0.0, A, false), window(52, 950.0, B, false)],
        vec![window(52, 0.0, A, false), window(31, 950.0, A, false)],
        vec![window(52, 0.0, A, true), window(31, 950.0, B, true)],
        vec![window(0, 0.0, A, false)],
        vec![window(4_294_967_296, 0.0, A, false)],
        vec![window(52, 0.0, &A.to_uppercase(), false)],
        vec![window(52, 0.0, "new", false)],
        vec![window(52, 0.0, &format!("{A}?hostId=x"), false)],
        vec![window(52, 0.0, &format!("codex://threads/{A}"), false)],
        vec![window(
            52,
            0.0,
            "01a00000_0000_4000_8000_00000000000a",
            false,
        )],
    ];
    for windows in invalid {
        let candidate = snapshot(windows.clone());
        assert!(
            WindowTaskSessionValidationService::snapshot(&candidate, &expected()).is_err(),
            "accepted {candidate}"
        );
    }
    let over_limit: Vec<Value> = (1..=65)
        .map(|index| {
            window(
                index,
                index as f64 * 10.0,
                &format!("01a00000-0000-4000-8000-{index:012x}"),
                false,
            )
        })
        .collect();
    assert!(
        WindowTaskSessionValidationService::snapshot(&snapshot(over_limit), &expected()).is_err()
    );
}

#[test]
fn snapshot_frames_and_fields_are_checked() {
    let mut small = window(52, 0.0, A, false);
    small["frame"]["width"] = json!(299.0);
    let mut short = window(52, 0.0, A, false);
    short["frame"]["height"] = json!(249.0);
    let mut textual = window(52, 0.0, A, false);
    textual["frame"]["x"] = json!("0");
    let mut extra_frame = window(52, 0.0, A, false);
    extra_frame["frame"]["display"] = json!(1);
    let mut extra = window(52, 0.0, A, false);
    extra["title"] = json!("a task title");
    let mut flag = window(52, 0.0, A, false);
    flag["focused"] = json!("yes");
    for window in [small, short, textual, extra_frame, extra, flag] {
        assert!(
            WindowTaskSessionValidationService::snapshot(
                &snapshot(vec![window.clone()]),
                &expected()
            )
            .is_err(),
            "accepted {window}"
        );
    }
    let mut unknown = snapshot(vec![window(52, 0.0, A, false)]);
    unknown["task_count"] = json!(1);
    assert!(WindowTaskSessionValidationService::snapshot(&unknown, &expected()).is_err());
    let mut unrestored = snapshot(vec![window(52, 0.0, A, false)]);
    unrestored["clipboard_restored"] = json!(null);
    assert!(WindowTaskSessionValidationService::snapshot(&unrestored, &expected()).is_err());
    let mut recycled = snapshot(vec![window(52, 0.0, A, false)]);
    recycled["process"]["birth_id"] = json!("1726789012:000008");
    assert_eq!(
        WindowTaskSessionValidationService::snapshot(&recycled, &expected()),
        Err("Window task helper process identity changed".into())
    );
}

#[test]
fn errors_never_carry_a_task_id() {
    let mut extra = window(52, 0.0, A, false);
    extra["task"] = json!(A);
    for candidate in [
        snapshot(vec![extra]),
        snapshot(vec![window(52, 0.0, A, false), window(31, 950.0, A, false)]),
        json!({"process": process(), "windows": [window(52, 0.0, A, false)], "clipboard_restored": true, "leak": A}),
    ] {
        let error =
            WindowTaskSessionValidationService::snapshot(&candidate, &expected()).unwrap_err();
        assert!(!error.contains(A), "{error}");
    }
}

fn restore(verified: Value) -> Value {
    json!({"process": process(), "verified": verified, "clipboard_restored": false})
}

#[test]
fn a_restore_report_must_cover_every_planned_window() {
    assert_eq!(
        WindowTaskSessionValidationService::restore(&restore(json!([true, false])), &expected(), 2),
        Ok(WindowTaskRestoreReport {
            verified: vec![true, false],
            clipboard_restored: false,
        })
    );
    for verified in [
        json!([true]),
        json!([true, true, true]),
        json!([true, "yes"]),
        json!([true, null]),
        json!("true"),
        json!(null),
    ] {
        assert!(
            WindowTaskSessionValidationService::restore(&restore(verified.clone()), &expected(), 2)
                .is_err(),
            "accepted {verified}"
        );
    }
    let mut extra = restore(json!([true]));
    extra["task_ids"] = json!([A]);
    assert!(WindowTaskSessionValidationService::restore(&extra, &expected(), 1).is_err());
    let mut ids = restore(json!([true]));
    ids["window_ids"] = json!([101]);
    assert!(WindowTaskSessionValidationService::restore(&ids, &expected(), 1).is_err());
}

#[test]
fn a_rehearsal_is_accepted_only_when_every_window_verified() {
    let rehearsal = |ids: Value, verified: Value| json!({"process": process(), "window_ids": ids, "verified_count": verified, "clipboard_restored": true});
    assert_eq!(
        WindowTaskSessionValidationService::rehearsal(
            &rehearsal(json!([31, 32]), json!(2)),
            &expected()
        ),
        Ok(WindowTaskReport {
            windows: 2,
            verified: 2,
            clipboard_restored: true
        })
    );
    for (ids, verified) in [
        (json!([31, 32]), json!(1)),
        (json!([31, 32]), json!(3)),
        (json!([]), json!(0)),
        (json!([31, 31]), json!(2)),
        (json!([31]), json!("1")),
    ] {
        assert!(WindowTaskSessionValidationService::rehearsal(
            &rehearsal(ids, verified),
            &expected()
        )
        .is_err());
    }
}

#[test]
fn restore_and_rehearsal_responses_must_name_the_exact_process() {
    let mut restore = restore(json!([true]));
    restore["process"]["birth_id"] = json!("1726789012:000008");
    assert_eq!(
        WindowTaskSessionValidationService::restore(&restore, &expected(), 1),
        Err("Window task helper process identity changed".into())
    );
    let rehearsal = json!({
        "process": {"pid": 4243, "birth_id": "1726789012:000007"},
        "window_ids": [31],
        "verified_count": 1,
        "clipboard_restored": true
    });
    assert_eq!(
        WindowTaskSessionValidationService::rehearsal(&rehearsal, &expected()),
        Err("Window task helper process identity changed".into())
    );
}

fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/window-tasks")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The same fixtures are encoded by the Swift helper's records in
/// `tests/CodexWindowTaskRecordsTests.swift`.
#[test]
fn the_helper_records_rust_validates_match_the_shared_fixtures() {
    let snapshot =
        WindowTaskSessionValidationService::snapshot(&fixture("snapshot.json"), &expected())
            .unwrap();
    assert_eq!(snapshot.window_ids(), vec![31, 32]);
    assert_eq!(snapshot.windows[1].task_id, B);
    assert!(snapshot.windows[1].focused);
    assert_eq!(
        WindowTaskSessionValidationService::restore(
            &fixture("restore-result.json"),
            &expected(),
            2
        ),
        Ok(WindowTaskRestoreReport {
            verified: vec![true, false],
            clipboard_restored: true,
        })
    );
    assert_eq!(
        WindowTaskSessionValidationService::rehearsal(
            &fixture("rehearsal-result.json"),
            &expected()
        ),
        Ok(WindowTaskReport {
            windows: 2,
            verified: 2,
            clipboard_restored: false
        })
    );
    // The plan Rust sends is the one the Swift test decodes.
    let recovery = [B.to_string()];
    assert_eq!(
        snapshot.restore_plan(Some(&recovery)),
        fixture("restore-plan.json")
    );
}
