use super::SystemWeeklyResetEnvironment;
use crate::auto_reset::weekly_reset_environment::WeeklyResetEnvironment;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::test_live_system::assert_forbidden;

/// The production environment spends a credit, reads usage over the network,
/// and drives Desktop recovery. A unit test that forgets the fake must stop at
/// each seam instead of acting on the developer's account or Desktop.
#[test]
fn unit_tests_cannot_reach_live_reset_effects() {
    let account = TestAccountSpec {
        id: "synthetic",
        email: "user@example.invalid",
        plan: "team",
        ..TestAccountSpec::default()
    }
    .build();
    let spend = account.clone();
    assert_forbidden("reset-credit service", move || {
        SystemWeeklyResetEnvironment
            .consume_reset_credit(&spend, "00000000-0000-4000-8000-000000000001")
    });
    assert_forbidden("usage service", move || {
        SystemWeeklyResetEnvironment.read_usage(&account)
    });
    assert_forbidden("Desktop task recovery", || {
        SystemWeeklyResetEnvironment.recover_threads(&["synthetic-task".to_string()])
    });
    assert_forbidden("process table (/bin/ps)", || {
        SystemWeeklyResetEnvironment.desktop_running()
    });
}
