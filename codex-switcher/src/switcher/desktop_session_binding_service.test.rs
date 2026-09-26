use super::DesktopSessionBindingService;
use crate::distribution::{DesktopAppSession, WindowProcessIdentity};
use std::cell::Cell;

#[test]
fn binds_the_exact_new_process_before_recovery_is_allowed() {
    let home = std::env::temp_dir().join(format!("codex-desktop-binding-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let process = WindowProcessIdentity::new(321, "123:456789").unwrap();
    let inspections = Cell::new(0);
    let bound = DesktopSessionBindingService::bind_with(
        &home,
        "account-a",
        321,
        || vec![321],
        |_| {
            inspections.set(inspections.get() + 1);
            Ok(process.clone())
        },
    )
    .unwrap();
    assert_eq!(bound, process);
    assert_eq!(inspections.get(), 2);
    let session = DesktopAppSession::load(&home.join("desktop-app-session.json")).unwrap();
    assert_eq!(session.account_id, "account-a");
    assert_eq!(session.cli_account_id.as_deref(), Some("account-a"));
    assert_eq!(session.process.as_ref(), Some(&process));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn changing_process_identity_never_writes_a_new_marker() {
    let home = std::env::temp_dir().join(format!(
        "codex-desktop-binding-reject-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let observations = Cell::new(0);
    assert!(DesktopSessionBindingService::bind_with(
        &home,
        "account-a",
        321,
        || vec![321],
        |_| {
            observations.set(observations.get() + 1);
            WindowProcessIdentity::new(
                321,
                if observations.get() == 1 {
                    "123:456789"
                } else {
                    "123:456790"
                },
            )
        },
    )
    .is_err());
    assert!(!home.join("desktop-app-session.json").exists());
    std::fs::remove_dir_all(home).unwrap();
}
