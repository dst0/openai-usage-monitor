#[test]
fn automatic_entrypoints_have_no_legacy_switch_bypass() {
    let daemon = concat!(
        include_str!("../src/daemon.rs"),
        include_str!("../src/distribution/daemon_tick_service.rs")
    );
    let shim = include_str!("../src/shim.rs");

    for (name, source) in [("daemon", daemon), ("wrapper", shim)] {
        assert!(
            !source.contains("switch_to_account"),
            "{name} must not call the legacy switch path"
        );
        assert!(
            !source.contains("select_best_switch"),
            "{name} must not select distribution targets"
        );
        assert!(
            !source.contains("dispatch_self_restart"),
            "{name} must not dispatch a detached legacy switch"
        );
    }
}

#[test]
fn automatic_entrypoints_use_the_distribution_service() {
    let daemon = concat!(
        include_str!("../src/daemon.rs"),
        include_str!("../src/distribution/daemon_tick_service.rs")
    );
    let shim = include_str!("../src/shim.rs");

    assert!(daemon.contains("AutomaticDistributionService"));
    assert!(shim.contains("AutomaticDistributionService"));
}
