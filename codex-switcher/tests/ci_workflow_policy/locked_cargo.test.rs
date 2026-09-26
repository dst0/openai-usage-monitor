use super::unlocked_cargo_violations;

fn violations(text: &str) -> Vec<String> {
    unlocked_cargo_violations(text)
}

#[test]
fn locked_or_frozen_invocations_comply() {
    for text in [
        "cargo test --locked\n",
        "cargo build --release --locked\n",
        "cargo test --locked --verbose\n",
        "cargo clippy --all-targets --locked -- -D warnings\n",
        "cargo test --frozen\n",
        "(cd codex-switcher && /usr/bin/env cargo build --locked --quiet --bin codex-mon)\n",
        "~/.cargo/bin/cargo test --locked\n",
        "\"${HOME}/.cargo/bin/cargo\" test --locked\n",
        "cargo build --locked && cargo test --locked\n",
        "      - run: cargo test --locked\n",
        "        run: 'cargo test --locked'\n",
        "cargo test --locked 2>&1 | tee test.log\n",
        // A continued command keeps the flag on a later line.
        "cargo test \\\n  --verbose \\\n  --locked\n",
    ] {
        assert_eq!(violations(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn invocations_that_cannot_touch_the_lockfile_comply() {
    for text in [
        "cargo --version\n",
        "cargo -V\n",
        "cargo --version >/dev/null 2>&1\n",
        "cargo --version > /dev/null\n",
        "cargo clippy --version 2> /dev/null\n",
        "  cargo clippy --version\n",
        "cargo fmt --check\n",
        "cargo fmt --all -- --check\n",
        "if ! command -v cargo >/dev/null 2>&1; then\n",
        "command -V cargo\n",
        "which cargo\n",
        "type -P cargo\n",
        "hash cargo 2>/dev/null\n",
        "rustup which cargo\n",
    ] {
        assert_eq!(violations(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn text_that_only_mentions_cargo_is_not_a_command() {
    for text in [
        "source \"${HOME}/.cargo/env\"\n",
        "            ~/.cargo/registry/index/\n",
        "          key: ${{ runner.os }}-cargo-${{ hashFiles('codex-switcher/Cargo.lock') }}\n",
        "      - name: Run cargo test\n",
        "    name: cargo build (release)\n",
        "# cargo test runs in CI\n",
        "          # cargo build\n",
        "cargo test --locked # cargo build without the flag is rejected\n",
        "echo \"Rust / Cargo not found.\"\n",
        "cargo2 test\n",
        "CARGO_TERM_COLOR: always\n",
    ] {
        assert_eq!(violations(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn unlocked_invocations_are_rejected() {
    for text in [
        "cargo test\n",
        "cargo test --verbose\n",
        "cargo build --release\n",
        "cargo clippy --all-targets -- -D warnings\n",
        "cargo check\n",
        "cargo run --bin codex-mon -- status\n",
        "cargo fetch\n",
        "cargo metadata --format-version 1\n",
        "cargo install --path .\n",
        // Commands that rewrite the lockfile are rejected like any other.
        "cargo generate-lockfile\n",
        "cargo update\n",
        "      - run: cargo test\n",
        "        run: \"cargo build\"\n",
        "(cd codex-switcher && /usr/bin/env cargo build --quiet --bin codex-mon)\n",
        "~/.cargo/bin/cargo test\n",
        "\"$HOME/.cargo/bin/cargo\" test\n",
        "command cargo test\n",
        "time cargo test\n",
        "env RUST_BACKTRACE=1 cargo test\n",
        "VERSION=$(cargo metadata --no-deps)\n",
        "cargo test >test.log 2>&1\n",
        // No subcommand is still an invocation the scan cannot prove harmless.
        "echo test | xargs cargo\n",
        // A redirection target is a file name, not a flag.
        "cargo build > --locked\n",
        "cargo build 2> --frozen\n",
        // Near misses of the flags.
        "cargo test --lockedx\n",
        "cargo test --locked=true\n",
        "cargo test --lock\n",
    ] {
        let v = violations(text);
        assert_eq!(v.len(), 1, "{text:?}: {v:?}");
        assert!(v[0].starts_with("line 1: `"), "{text:?}: {v:?}");
        assert!(v[0].contains("must pass `--locked`"), "{v:?}");
    }
}

/// `cargo test -- --locked` passes the flag to the test binary; cargo itself
/// still creates or rewrites the lockfile before running it.
#[test]
fn flags_after_a_bare_double_dash_belong_to_the_program() {
    for text in [
        "cargo test -- --locked\n",
        "cargo run -- --frozen\n",
        "cargo clippy -- --locked -D warnings\n",
    ] {
        assert_eq!(violations(text).len(), 1, "{text:?}");
    }
    assert_eq!(
        violations("cargo test --locked -- --nocapture\n"),
        Vec::<String>::new()
    );
}

/// Exemptions match whole argument lists, so a version or `fmt` word in
/// another position cannot excuse a build.
#[test]
fn exemptions_require_their_exact_shape() {
    for text in [
        "cargo test --version\n",
        "cargo clippy --version --all-targets\n",
        "cargo --version test\n",
        "cargo -C fmt build\n",
        "cargo build fmt\n",
        "cargo -vV\n",
    ] {
        assert_eq!(violations(text).len(), 1, "{text:?}");
    }
}

#[test]
fn each_simple_command_is_checked_on_its_own() {
    for text in [
        "cargo build --locked && cargo test\n",
        "cargo build --locked; cargo test\n",
        "cargo build --locked || cargo test\n",
        "cargo build --locked | cargo test\n",
        "cargo test & cargo build --locked\n",
        "echo `cargo metadata` && cargo build --locked\n",
    ] {
        let v = violations(text);
        assert_eq!(v.len(), 1, "{text:?}: {v:?}");
    }
    let both = violations("cargo build && cargo test\n");
    assert_eq!(both.len(), 2, "{both:?}");
    assert!(both[0].contains("`cargo build`") && both[1].contains("`cargo test`"));
}

#[test]
fn lookups_do_not_excuse_invocations() {
    for text in [
        "command -v cargo && cargo build\n",
        "which cargo; cargo test\n",
        "command -p cargo build\n",
    ] {
        assert_eq!(violations(text).len(), 1, "{text:?}");
    }
}

#[test]
fn violations_name_the_first_line_of_a_continued_command() {
    let text = "run: |\n  rustup toolchain install\n  cargo test \\\n    --verbose\n  cargo build --locked\n  cargo clippy\n";
    assert_eq!(
        violations(text),
        vec![
            "line 3: `cargo test --verbose` must pass `--locked` so a missing or stale Cargo.lock fails instead of being re-resolved",
            "line 6: `cargo clippy` must pass `--locked` so a missing or stale Cargo.lock fails instead of being re-resolved",
        ]
    );
}

/// The live CI workflow before this rule: it re-resolved the lockfile and
/// tested without `--locked`.
#[test]
fn a_workflow_that_resolves_its_own_lockfile_is_rejected() {
    let text = "      - name: Resolve Dependencies\n        working-directory: codex-switcher\n        run: cargo generate-lockfile\n\n      - name: Run Rust Unit Tests\n        working-directory: codex-switcher\n        run: cargo test --verbose\n";
    let v = violations(text);
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(
        v[0].starts_with("line 3: `cargo generate-lockfile`"),
        "{v:?}"
    );
    assert!(v[1].starts_with("line 7: `cargo test --verbose`"), "{v:?}");
}

/// `workflow_violations` runs this rule, so the live workflow test cannot pass
/// with an unlocked cargo step.
#[test]
fn workflow_violations_include_the_locked_cargo_rule() {
    let unlocked = crate::fixtures::with("run: cargo test --locked", "run: cargo test");
    assert_eq!(
        crate::rules::workflow_violations(&unlocked),
        ["line 20: `cargo test` must pass `--locked` so a missing or stale Cargo.lock fails instead of being re-resolved"]
    );
}
