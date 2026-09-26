use super::Settings;

#[test]
fn new_and_legacy_registries_keep_automatic_switching_disabled() {
    assert!(!Settings::default().auto_switch_enabled);
    let missing: Settings = serde_json::from_str("{}").unwrap();
    assert!(!missing.auto_switch_enabled);
    let explicit: Settings = serde_json::from_str("{\"auto_switch_enabled\":true}").unwrap();
    assert!(explicit.auto_switch_enabled);
}
