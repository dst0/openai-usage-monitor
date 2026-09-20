use super::automation_guard::{
    arm_automation_cooldown_at, automation_cooldown_remaining_at, claim_restart_operation_at,
    cooldown_path, valid_operation_id,
};
use std::{
    os::unix::fs::PermissionsExt,
    time::{Duration, UNIX_EPOCH},
};

#[test]
fn restart_operation_can_be_claimed_only_once() {
    let root = std::env::temp_dir().join(format!(
        "codex-restart-claim-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    std::fs::create_dir_all(&root).unwrap();
    assert!(claim_restart_operation_at(&root, "12345-678").unwrap());
    assert!(!claim_restart_operation_at(&root, "12345-678").unwrap());
    let metadata = std::fs::metadata(root.join("recovery-runs/restart-12345-678.claimed")).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn restart_operation_rejects_path_components() {
    assert!(!valid_operation_id("../worker"));
    assert!(!valid_operation_id("worker"));
    assert!(valid_operation_id("12345-678"));
}

#[test]
fn durable_automation_cooldown_is_atomic_private_and_expires() {
    let root = std::env::temp_dir().join(format!(
        "codex-cooldown-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    std::fs::create_dir_all(&root).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
    arm_automation_cooldown_at(&root, now, Duration::from_secs(180)).unwrap();
    assert_eq!(
        automation_cooldown_remaining_at(&root, now).unwrap(),
        Some(Duration::from_secs(180))
    );
    assert_eq!(
        std::fs::metadata(cooldown_path(&root))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        automation_cooldown_remaining_at(&root, now + Duration::from_secs(180)).unwrap(),
        None
    );
    std::fs::write(cooldown_path(&root), b"invalid\n").unwrap();
    assert!(automation_cooldown_remaining_at(&root, now).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
