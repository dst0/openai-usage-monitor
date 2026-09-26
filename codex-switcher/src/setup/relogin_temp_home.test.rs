use super::relogin_temp_home::ReloginTempHome;
use std::os::unix::fs::PermissionsExt;

#[test]
fn login_homes_are_unique_private_and_clean_up_only_their_own_files() {
    let first = ReloginTempHome::new().unwrap();
    let marker = first.path().join("auth.json");
    std::fs::write(&marker, b"fixture").unwrap();
    let second = ReloginTempHome::new().unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(
        std::fs::metadata(first.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(second.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"fixture");
    let first_path = first.path().to_path_buf();
    let second_path = second.path().to_path_buf();
    drop(second);
    assert!(marker.exists());
    assert!(!second_path.exists());
    drop(first);
    assert!(!first_path.exists());
}
