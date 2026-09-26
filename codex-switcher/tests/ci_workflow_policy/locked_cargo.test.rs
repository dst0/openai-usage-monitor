use super::{cargo_commands, runs_locked, unlocked_cargo_violations};

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
    let line = 1 + unlocked
        .lines()
        .position(|l| l.ends_with("run: cargo test"))
        .expect("fixture step");
    assert_eq!(
        crate::rules::workflow_violations(&unlocked),
        [format!("line {line}: `cargo test` must pass `--locked` so a missing or stale Cargo.lock fails instead of being re-resolved")]
    );
}

/// Regression (review of this PR, F1): comments were cut at the first ` #`,
/// so a tab before `#` kept the comment text and a quoted `#` hid the rest of
/// the line.
#[test]
fn quoted_hashes_and_tab_comments_do_not_hide_commands() {
    for (text, command) in [
        ("cargo build\t# TODO\n", "`cargo build`"),
        // The old reader kept this comment and counted its `--locked`.
        ("cargo build\t# --locked\n", "`cargo build`"),
        ("echo \"Building #1\" && cargo build\n", "`cargo build`"),
        ("run: echo \"step #2\"; cargo test\n", "`cargo test`"),
        ("echo 'a # b' && cargo test\n", "`cargo test`"),
    ] {
        let v = violations(text);
        assert_eq!(v.len(), 1, "{text:?}: {v:?}");
        assert!(v[0].contains(command), "{text:?}: {v:?}");
    }
    assert_eq!(
        violations("cargo build --locked\t# cargo test\n"),
        Vec::<String>::new()
    );
}

#[test]
fn an_unterminated_quote_is_reported() {
    let v = violations("cargo test --locked\necho \"unclosed\ncargo build --locked\n");
    assert_eq!(
        v,
        ["line 2: quote is never closed, so the cargo scan cannot tell commands from quoted text"]
    );
    // Quoted programs that span lines and close are read normally.
    let awk = "awk '\n  { print }\n' file\ncargo test --locked\n";
    assert_eq!(violations(awk), Vec::<String>::new());
}

/// Regression (review of this PR, F2): a backslash, quotes inside the word, a
/// looked-up path, or a variable in the command position still run cargo.
#[test]
fn spellings_that_still_run_cargo_are_invocations() {
    for text in [
        "\\cargo build\n",
        "c\\argo build\n",
        "ca''rgo build\n",
        "$(which cargo) build\n",
        "$(command -v cargo) build\n",
        "\"$(command -v cargo)\" build\n",
        "`which cargo` build\n",
        "$CARGO build\n",
        "\"${CARGO}\" test\n",
        "${CARGO_BIN:-cargo} test\n",
        "RUSTFLAGS=-Dwarnings $MY_CARGO clippy\n",
        "env -i $cargo test\n",
        "if ! $CARGO build; then\n",
        "eval \"cargo build\"\n",
    ] {
        let v = violations(text);
        assert_eq!(v.len(), 1, "{text:?}: {v:?}");
    }
    for text in [
        "$(command -v cargo) build --locked\n",
        "\"$CARGO\" test --locked\n",
        "CARGO=$(command -v cargo)\n",
        "export CARGO=\"$(command -v cargo)\"\n",
        "echo \"$CARGO_HOME\"\n",
        "rm -rf \"${CARGO_TARGET_DIR}\"\n",
        "[ -d \"$CARGO_HOME\" ]\n",
        "PATH=\"$CARGO_HOME/bin:$PATH\"\n",
        "$(command -v rustc) --version\n",
        // A path under a CARGO variable is a program of its own.
        "\"$CARGO_HOME/bin/rustup\" show\n",
    ] {
        assert_eq!(violations(text), Vec::<String>::new(), "{text:?}");
    }
}

/// Review of this PR, F4: shapes around separators and continuations.
#[test]
fn separators_and_continuations_bound_each_command() {
    for (text, count) in [
        ("cargo build \\\n", 1),
        ("cargo test | cargo build --locked\n", 1),
        ("cargo test || cargo build --locked\n", 1),
        ("(cargo test) --locked\n", 1),
        // bash ends the continued command at the comment and runs it unlocked.
        ("cargo test \\\n  # c\n  --locked\n", 1),
    ] {
        let v = violations(text);
        assert_eq!(v.len(), count, "{text:?}: {v:?}");
    }
}

#[test]
fn cargo_commands_start_at_the_word_that_runs_cargo() {
    assert_eq!(
        cargo_commands("x=1 \\\n  /usr/bin/env cargo build --locked # b\nwhich cargo\n"),
        [(
            1,
            vec![
                "cargo".to_string(),
                "build".to_string(),
                "--locked".to_string()
            ]
        )]
    );
}

#[test]
fn runs_locked_requires_the_subcommand_and_its_own_flag() {
    let script = "cargo build --release --locked\ncargo test\n";
    assert!(runs_locked(script, "build"));
    assert!(!runs_locked(script, "test"));
    assert!(!runs_locked("cargo build --release\n", "build"));
    assert!(runs_locked("cargo test --frozen\n", "test"));
    for text in [
        "cargo test -- --locked\n",
        "cargo clippy --locked && cargo test\n",
        "cargo --locked test\n",
        "# cargo test --locked\n",
    ] {
        assert!(!runs_locked(text, "test"), "{text:?}");
    }
    // Prose still reads as a command (fail closed for the negative rule), so
    // the live check binds to a required job's own text.
    assert!(runs_locked("echo cargo test --locked\n", "test"));
}
