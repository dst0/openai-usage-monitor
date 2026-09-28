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

#[test]
fn the_daemon_keeps_one_automatic_backoff_across_ticks() {
    let daemon_loop = include_str!("../src/distribution/daemon_loop_service.rs");
    let tick = include_str!("../src/distribution/daemon_tick_service.rs");
    let created = daemon_loop
        .find("Arc::new(AutomaticDistributionBackoff::default())")
        .expect("the daemon loop must own the automatic-distribution backoff");
    let tick_loop = daemon_loop
        .find("loop {")
        .expect("the daemon loop must have a tick loop");
    assert!(
        created < tick_loop,
        "a backoff made per tick would forget every failure"
    );
    assert!(daemon_loop.contains("DaemonTickService::run(true, Some(&automatic_backoff))"));
    assert!(tick.contains("coordinator.with_automatic_backoff(backoff.clone())"));
}
