use super::DesktopSessionVerificationService;
use crate::distribution::desktop_app_session::DesktopAppSession;
use crate::distribution::desktop_external_binding_service::DesktopExternalBindingService;
use crate::distribution::mock_app_lifecycle::MockAppLifecycle;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{read_active_auth_json, write_active_auth_json};

#[test]
fn cli_binding_preserves_inferred_app_provenance_and_rejects_replaced_auth() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("inferred_binding_provenance");
    let account = TestAccountSpec {
        id: "active",
        email: "active@example.com",
        plan: "plus",
        sprint_pct: 50.0,
        ..TestAccountSpec::default()
    }
    .build();
    env.populate(vec![account.clone()], Some("active"), Some("active"));
    let marker_path = env.home().join("desktop-app-session.json");
    let mut marker = DesktopAppSession::load(&marker_path).unwrap();
    marker.account_id = account.id.clone();
    marker.cli_account_id = Some(account.id.clone());
    marker.auth_file_id = Some(
        DesktopExternalBindingService::read_auth_evidence(env.home())
            .unwrap()
            .1,
    );
    marker.save(&marker_path).unwrap();
    let lifecycle = MockAppLifecycle::new(true);
    let verifier = DesktopSessionVerificationService::new(&lifecycle, env.home());

    verifier.reconcile_cli_binding("active").unwrap();
    assert_eq!(
        DesktopAppSession::load(&marker_path).unwrap().auth_file_id,
        marker.auth_file_id
    );

    let mut changed_auth = read_active_auth_json().unwrap();
    changed_auth.tokens.as_mut().unwrap().access_token = "different-test-token".into();
    write_active_auth_json(&changed_auth).unwrap();
    assert!(verifier.verified_session().is_err());
    assert!(verifier.reconcile_cli_binding("active").is_err());
    assert_eq!(
        DesktopAppSession::load(&marker_path).unwrap().auth_file_id,
        marker.auth_file_id
    );
}
