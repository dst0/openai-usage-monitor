//! `CODEX_HOME` is process-wide. A test that sets it by hand can leave it
//! pointing at a deleted directory, and one that removes it makes later code
//! resolve the owner's live `~/.codex`. Only `TestCodexHome`, which serializes
//! access and restores the previous value while unwinding, may change it.
//!
//! Only that guard may lock `TEST_CODEX_HOME_MUTEX`, too. The mutex is not
//! reentrant, and the guard's nested-use check sees only other guards: a test
//! that locks it directly and then creates a guard (for example through
//! `TestEnv`) blocks forever instead of failing.

use std::fs;
use std::path::{Path, PathBuf};

const GUARD_FILES: [&str; 2] = [
    "storage/test_codex_home.rs",
    "storage/test_codex_home.test.rs",
];
// Built at runtime so this file does not match its own patterns.
const FORBIDDEN_CALLS: [&str; 2] = ["set_var(", "remove_var("];
const KEY: &str = "CODEX_HOME";
const SERIAL_LOCK: [&str; 2] = ["TEST_CODEX_HOME_", "MUTEX.lock("];

#[test]
fn only_the_test_guard_changes_codex_home() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = crate_root.join("src");
    let mut violations = Vec::new();
    let mut scanned = 0;
    let files = rust_files(&src)
        .into_iter()
        .chain(rust_files(&crate_root.join("tests")));
    for path in files {
        let relative = path.strip_prefix(&src).unwrap_or(&path);
        if GUARD_FILES.iter().any(|guard| relative == Path::new(guard)) {
            continue;
        }
        scanned += 1;
        // rustfmt may wrap the arguments, so compare without whitespace.
        let compact: String = fs::read_to_string(&path)
            .unwrap()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        for call in FORBIDDEN_CALLS {
            let call = format!("{call}\"{KEY}\"");
            if compact.contains(&call) {
                violations.push(format!("{} calls {call}…)", relative.display()));
            }
        }
        let serial_lock = SERIAL_LOCK.concat();
        if compact.contains(&serial_lock) {
            violations.push(format!(
                "{} calls {serial_lock}…) outside the guard",
                relative.display()
            ));
        }
    }

    assert!(
        scanned > 100,
        "expected to scan the crate, saw {scanned} files"
    );
    assert!(
        violations.is_empty(),
        "use storage::test_codex_home::TestCodexHome instead of changing CODEX_HOME directly:\n{}",
        violations.join("\n")
    );
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files
}
