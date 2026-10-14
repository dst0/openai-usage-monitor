use super::invoked_as_codex;

#[test]
fn a_bare_codex_from_path_runs_the_shim() {
    // Regression: shells pass the bare command word as argv[0], so the old
    // `ends_with("/codex")` check parsed `codex resume <id>` as a cxi command.
    assert!(invoked_as_codex("codex"));
    assert!(invoked_as_codex("/Users/me/.local/bin/codex"));
    assert!(invoked_as_codex("./codex"));
}

#[test]
fn other_invocations_are_cxi_commands() {
    for arg0 in ["cxi", "codex-mon", "/usr/local/bin/cxi", "codex-cli", "xcodex", "", "/"] {
        assert!(!invoked_as_codex(arg0), "{arg0}");
    }
}
