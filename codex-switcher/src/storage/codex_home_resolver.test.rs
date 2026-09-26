use super::{check_test_home, configured_codex_home, live_codex_home};
use std::path::{Path, PathBuf};
use std::thread::ThreadId;

#[test]
fn only_a_non_empty_codex_home_is_an_explicit_choice() {
    assert_eq!(configured_codex_home(None), None);
    assert_eq!(configured_codex_home(Some(String::new())), None);
    assert_eq!(
        configured_codex_home(Some("/tmp/codex-a".into())),
        Some(PathBuf::from("/tmp/codex-a"))
    );
}

#[test]
fn live_home_is_dot_codex_under_the_user_home() {
    assert_eq!(
        live_codex_home(Some("/Users/owner".into())),
        Path::new("/Users/owner/.codex")
    );
    assert_eq!(live_codex_home(None), Path::new(".codex"));
}

fn other_thread() -> ThreadId {
    std::thread::spawn(|| std::thread::current().id())
        .join()
        .unwrap()
}

#[test]
fn test_builds_accept_only_the_calling_threads_guard_home() {
    let me = std::thread::current().id();
    let guard = PathBuf::from("/private/tmp/codex-test-home-a");
    let active = || Some((guard.clone(), me));

    assert!(check_test_home(Some(&guard), active(), me).is_ok());
    // A trailing separator names the same directory.
    let trailing = Path::new("/private/tmp/codex-test-home-a/");
    assert!(check_test_home(Some(trailing), active(), me).is_ok());

    let unset = check_test_home(None, active(), me).unwrap_err();
    assert!(unset.contains("CODEX_HOME is unset"), "{unset}");

    // The live home, an inherited shell value, or a symlink to either is
    // rejected because it is not the guard's own directory.
    for foreign in [
        "/Users/owner/.codex",
        "/Users/owner/.CODEX",
        "/tmp/link-to-codex",
    ] {
        let error = check_test_home(Some(Path::new(foreign)), active(), me).unwrap_err();
        assert!(error.contains("is not the guard's home"), "{error}");
    }
}

#[test]
fn test_builds_refuse_a_home_nobody_owns_or_another_thread_owns() {
    let me = std::thread::current().id();
    let path = PathBuf::from("/private/tmp/codex-test-home-b");

    let unowned = check_test_home(Some(&path), None, me).unwrap_err();
    assert!(unowned.contains("no test owns CODEX_HOME"), "{unowned}");

    let foreign_owner = Some((path.clone(), other_thread()));
    let borrowed = check_test_home(Some(&path), foreign_owner, me).unwrap_err();
    assert!(borrowed.contains("another test's thread"), "{borrowed}");
}
