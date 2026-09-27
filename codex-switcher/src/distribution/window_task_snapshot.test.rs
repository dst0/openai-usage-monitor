use super::*;
use crate::distribution::window_restore_frame::WindowFrame;

fn entry(id: u32, x: f64, task: &str, focused: bool) -> WindowTaskEntry {
    WindowTaskEntry {
        window_id: id,
        frame: WindowFrame {
            x,
            y: 30.0,
            width: 900.0,
            height: 700.0,
        },
        task_id: task.into(),
        focused,
    }
}

fn snapshot(windows: Vec<WindowTaskEntry>) -> WindowTaskSnapshot {
    WindowTaskSnapshot {
        process: ProcessIdentity::new(4242, "1726789012:000007").unwrap(),
        windows,
    }
}

const A: &str = "01a00000-0000-4000-8000-00000000000a";
const B: &str = "01a00000-0000-4000-8000-00000000000b";

#[test]
fn window_ids_are_sorted_for_the_shutdown_guard() {
    let snapshot = snapshot(vec![entry(52, 0.0, A, false), entry(31, 950.0, B, true)]);
    assert_eq!(snapshot.window_ids(), vec![31, 52]);
}

#[test]
fn the_plan_keeps_capture_order_and_names_the_focused_window() {
    let snapshot = snapshot(vec![entry(52, 0.0, A, false), entry(31, 950.0, B, true)]);
    assert_eq!(
        snapshot.restore_plan(None),
        serde_json::json!({
            "windows": [
                {"task_id": A, "frame": {"x": 0.0, "y": 30.0, "width": 900.0, "height": 700.0}},
                {"task_id": B, "frame": {"x": 950.0, "y": 30.0, "width": 900.0, "height": 700.0}},
            ],
            "focus_index": 1,
            "mode": "relaunch",
            "recovery_task_ids": [],
        })
    );
    let recovery = [B.to_string()];
    let recheck = snapshot.restore_plan(Some(&recovery));
    assert_eq!(recheck["mode"], "recheck");
    assert_eq!(recheck["recovery_task_ids"], serde_json::json!([B]));
    let unfocused = super::WindowTaskSnapshot {
        windows: vec![entry(52, 0.0, A, false)],
        ..snapshot
    };
    assert_eq!(
        unfocused.restore_plan(None)["focus_index"],
        serde_json::Value::Null
    );
}

#[test]
fn debug_output_never_contains_a_task_id() {
    let snapshot = snapshot(vec![entry(52, 0.0, A, true)]);
    let debug = format!("{snapshot:?}");
    assert!(!debug.contains(A), "{debug}");
    assert!(debug.contains("<task>") && debug.contains("52"));
}
