use super::{
    choose_recovery_account_binding, session_matches_binding, session_matches_process_lifetime,
    verified_with,
};
use crate::distribution::{DesktopAppSession, WindowProcessIdentity};
use chrono::{Duration, Utc};
use std::cell::Cell;

#[test]
fn deferred_binding_selector_uses_desktop_id_after_verification() {
    assert_eq!(
        choose_recovery_account_binding(Some("cli-account"), Some("desktop-account"), true),
        Some("desktop-account".to_string())
    );
    assert_eq!(
        choose_recovery_account_binding(Some("cli-account"), None, true),
        None
    );
    assert_eq!(
        choose_recovery_account_binding(Some("cli-account"), Some("desktop-account"), false),
        Some("cli-account".to_string())
    );
}

#[test]
fn stale_or_invalid_desktop_session_cannot_bind_a_new_process() {
    let now = Utc::now();
    let birth = format!("{}:000000", (now - Duration::seconds(60)).timestamp());
    assert!(session_matches_process_lifetime(
        &(now - Duration::seconds(30)).to_rfc3339(),
        &birth
    ));
    assert!(!session_matches_process_lifetime(
        &(now - Duration::seconds(90)).to_rfc3339(),
        &birth
    ));
    assert!(!session_matches_process_lifetime(
        &(now + Duration::seconds(30)).to_rfc3339(),
        &birth
    ));
    assert!(!session_matches_process_lifetime(
        &now.to_rfc3339(),
        "invalid"
    ));
}

#[test]
fn managed_session_requires_exact_process_birth_and_expected_cli_account() {
    let process = WindowProcessIdentity::new(9999, "123:456789").unwrap();
    let session = DesktopAppSession::bound("desktop-account", "cli-account", process.clone());
    assert!(session_matches_binding(
        &session,
        &process,
        Some("cli-account")
    ));
    assert!(!session_matches_binding(
        &session,
        &process,
        Some("other-cli")
    ));
    assert!(!session_matches_binding(&session, &process, None));
    assert!(!session_matches_binding(
        &session,
        &WindowProcessIdentity::new(9999, "123:456790").unwrap(),
        Some("cli-account")
    ));
    assert!(!session_matches_binding(
        &DesktopAppSession::new("desktop-account"),
        &process,
        Some("cli-account")
    ));
}

#[test]
fn deferred_binding_checks_live_process_and_saved_session_at_both_reads() {
    let birth = format!(
        "{}:000000",
        (Utc::now() - Duration::seconds(60)).timestamp()
    );
    let process = WindowProcessIdentity::new(9999, birth).unwrap();
    let session = DesktopAppSession::bound("desktop-account", "cli-account", process.clone());
    let checks = Cell::new(0);
    assert_eq!(
        verified_with(
            Some("cli-account"),
            || vec![9999],
            |_| {
                checks.set(checks.get() + 1);
                Some(process.clone())
            },
            || Some(session.clone()),
            |id| id == "desktop-account",
        ),
        Some("desktop-account".into())
    );
    assert_eq!(checks.get(), 2);
    assert!(verified_with(
        Some("cli-account"),
        || vec![9999, 10000],
        |_| panic!("multiple Desktop processes must stop inspection"),
        || Some(session.clone()),
        |_| true,
    )
    .is_none());
    assert!(verified_with(
        Some("cli-account"),
        || vec![9999],
        |_| Some(process.clone()),
        || Some(session.clone()),
        |_| false,
    )
    .is_none());
    assert!(verified_with(
        Some("cli-account"),
        || vec![9999],
        |_| None,
        || Some(session.clone()),
        |_| true,
    )
    .is_none());
    assert!(verified_with(
        Some("cli-account"),
        || vec![9999],
        |_| Some(process.clone()),
        || None,
        |_| true,
    )
    .is_none());
    let reads = Cell::new(0);
    assert!(verified_with(
        Some("cli-account"),
        || vec![9999],
        |_| {
            reads.set(reads.get() + 1);
            if reads.get() == 1 {
                Some(process.clone())
            } else {
                Some(WindowProcessIdentity::new(9999, "1:000000").unwrap())
            }
        },
        || Some(session.clone()),
        |_| true,
    )
    .is_none());
    let pid_reads = Cell::new(0);
    assert!(verified_with(
        Some("cli-account"),
        || {
            pid_reads.set(pid_reads.get() + 1);
            vec![if pid_reads.get() == 1 { 9999 } else { 10000 }]
        },
        |_| Some(process.clone()),
        || Some(session.clone()),
        |_| true,
    )
    .is_none());
}
