use crate::models::DesktopWindowBounds;

#[test]
fn desktop_window_bounds_roundtrip_and_defaults() {
    let json = r#"{"version":1,"x":-2561.0,"y":-139.0,"width":1281.0,"height":1410.0,"updated_at":1789388770}"#;
    let bounds: DesktopWindowBounds = serde_json::from_str(json).unwrap();
    assert_eq!(bounds.x, -2561.0);
    assert_eq!(bounds.y, -139.0);
    assert_eq!(bounds.width, 1281.0);
    assert_eq!(bounds.height, 1410.0);
    assert_eq!(bounds.version, 1);

    let without_version = r#"{"x":100.0,"y":200.0,"width":800.0,"height":600.0}"#;
    let parsed: DesktopWindowBounds = serde_json::from_str(without_version).unwrap();
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.width, 800.0);

    let serialized = serde_json::to_string(&bounds).unwrap();
    let back: DesktopWindowBounds = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, bounds);
}

#[test]
fn settings_preserve_window_bounds_defaults_to_true() {
    let settings = crate::models::Settings::default();
    assert!(settings.preserve_window_bounds_on_restart);

    let json = r#"{}"#;
    let parsed: crate::models::Settings = serde_json::from_str(json).unwrap();
    assert!(parsed.preserve_window_bounds_on_restart);

    let json_disabled = r#"{"preserve_window_bounds_on_restart":false}"#;
    let parsed_disabled: crate::models::Settings = serde_json::from_str(json_disabled).unwrap();
    assert!(!parsed_disabled.preserve_window_bounds_on_restart);
}
