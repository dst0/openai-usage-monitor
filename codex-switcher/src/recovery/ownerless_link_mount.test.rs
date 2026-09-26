use super::open_then_wait_for_owner;
use crate::{recovery::ipc_call_error::IpcCallError, storage::test_codex_home::TestCodexHome};

#[test]
fn initial_url_delivery_error_still_waits_for_owner_without_dispatch() {
    let _home = TestCodexHome::new("ownerless-initial-link");
    let mut owner_probes = 0;
    let result: Result<(), IpcCallError> = open_then_wait_for_owner(
        || Err("ordinary task URL delivery failed".into()),
        || {
            owner_probes += 1;
            Err(IpcCallError::NoClientFound)
        },
    );
    assert!(matches!(result, Err(IpcCallError::NoClientFound)));
    assert_eq!(owner_probes, 1);
}

#[test]
fn changed_desktop_identity_never_waits_or_dispatches() {
    let result: Result<(), IpcCallError> = open_then_wait_for_owner(
        || Err("ChatGPT process identity changed during task navigation".into()),
        || panic!("changed Desktop identity must stop owner routing"),
    );
    assert!(matches!(result, Err(IpcCallError::Other(_))));
}
