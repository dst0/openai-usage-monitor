use super::{cache_key_input_violations, hash_files_calls};
use crate::scratch_git_repo::ScratchGitRepo;

const KEY: &str = "          key: ${{ runner.os }}-cargo-${{ hashFiles('codex-switcher/rust-toolchain.toml') }}-${{ hashFiles('codex-switcher/Cargo.lock') }}\n";

fn paths(line: &str) -> Vec<Result<Vec<String>, String>> {
    hash_files_calls(line)
}

fn ok(paths: &[&str]) -> Result<Vec<String>, String> {
    Ok(paths.iter().map(|p| p.to_string()).collect())
}

#[test]
fn literal_arguments_are_read_from_every_call() {
    assert_eq!(
        paths(KEY),
        [
            ok(&["codex-switcher/rust-toolchain.toml"]),
            ok(&["codex-switcher/Cargo.lock"])
        ]
    );
    assert_eq!(
        paths("key: ${{ hashFiles('a/Cargo.lock', 'b/Cargo.lock') }}"),
        [ok(&["a/Cargo.lock", "b/Cargo.lock"])]
    );
    // Expression function names are case-insensitive.
    assert_eq!(
        paths("key: ${{ HashFiles('Cargo.lock') }}"),
        [ok(&["Cargo.lock"])]
    );
    assert_eq!(paths("key: ${{ runner.os }}-cargo"), Vec::new());
}

#[test]
fn patterns_and_unreadable_arguments_are_rejected() {
    for line in [
        "key: ${{ hashFiles('**/Cargo.lock') }}",
        "key: ${{ hashFiles('Cargo.?ock') }}",
        "key: ${{ hashFiles('[C]argo.lock') }}",
        "key: ${{ hashFiles('Cargo.lock', '!target/Cargo.lock') }}",
        "key: ${{ hashFiles(\"Cargo.lock\") }}",
        "key: ${{ hashFiles(Cargo.lock) }}",
        "key: ${{ hashFiles('it''s.lock') }}",
        "key: ${{ hashFiles('') }}",
        "key: ${{ hashFiles() }}",
        "key: ${{ hashFiles('Cargo.lock'",
    ] {
        let calls = paths(line);
        assert_eq!(calls.len(), 1, "{line}");
        assert!(calls[0].is_err(), "{line}: {calls:?}");
    }
}

/// Regression (docs/leanings/2026-09-26-ci-cargo-cache-key-hashed-ignored-lockfile.md):
/// the key hashed a gitignored `Cargo.lock`, which a clean checkout lacks, so
/// `hashFiles` returned an empty string and the key never changed.
#[test]
fn keys_must_hash_committed_files() {
    let scratch = ScratchGitRepo::init();
    scratch
        .write(".gitignore", "*.lock\n")
        .write("codex-switcher/rust-toolchain.toml", "[toolchain]\n")
        .commit_all(false)
        .write("codex-switcher/Cargo.lock", "version = 4\n");
    let v = cache_key_input_violations(KEY, &scratch.repo());
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(
        v[0].starts_with(
            "line 1: `hashFiles('codex-switcher/Cargo.lock')` names a file that is not committed"
        ),
        "{v:?}"
    );

    scratch.write(".gitignore", "").commit_all(false);
    assert_eq!(
        cache_key_input_violations(KEY, &scratch.repo()),
        Vec::<String>::new()
    );
}

#[test]
fn unreadable_calls_are_reported_with_their_line() {
    let scratch = ScratchGitRepo::init();
    scratch.write("Cargo.lock", "").commit_all(false);
    let text = "a: 1\nkey: ${{ hashFiles('**/Cargo.lock') }}\n";
    let v = cache_key_input_violations(text, &scratch.repo());
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(
        v[0].starts_with("line 2: `hashFiles('**/Cargo.lock')` is a pattern"),
        "{v:?}"
    );
}
