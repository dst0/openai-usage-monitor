//! Unit tests must never touch the owner's live desktop: the installed
//! Monitor helpers in `~/.local/bin`, or the live process table. Each such
//! seam calls [`forbid`] first in test builds, before it resolves a path or
//! spawns anything, so a test that forgets to inject a fake fails at the seam
//! instead of quietly depending on (or acting on) the developer's machine.
//! Production builds compile these calls out.

/// Fails the calling test. Not declared `-> !`, so the seam's own code after
/// the call still type-checks without unreachable-code warnings.
pub(crate) fn forbid(seam: &str) {
    panic!("a unit test reached the live {seam}; inject a fake instead");
}

/// Asserts that `reach` stops at the tripwire for `seam`. A seam whose
/// tripwire was removed returns instead, and this assertion fails.
pub(crate) fn assert_forbidden<T>(seam: &str, reach: impl FnOnce() -> T + std::panic::UnwindSafe) {
    let Err(panic) = std::panic::catch_unwind(reach) else {
        panic!("the live {seam} is reachable from unit tests");
    };
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        message.contains(&format!("the live {seam};")),
        "unexpected panic at the live {seam}: {message}"
    );
}
