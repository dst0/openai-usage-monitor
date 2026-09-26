# 2026-09-26 — rustup toolchain pin ignores `--manifest-path`

- **Status:** Resolved
- **Task/context:** Add `codex-switcher/rust-toolchain.toml` and prove CI and scripts build with the pinned release.
- **Unexpected observation or failure:** `cargo --manifest-path codex-switcher/Cargo.toml` run from another directory used the caller's default toolchain, not the pinned one.
- **Evidence:** A test printing `RUSTUP_TOOLCHAIN` saw `1.98.1-aarch64-apple-darwin` when cargo ran inside `codex-switcher/` and `1.94.1-aarch64-apple-darwin` when run from the scratchpad via `--manifest-path`. `tests/log_permissions_and_uninstall.sh` used the latter form, alternating toolchains in the shared `target/`.
- **Approaches tried:**
  - **Attempt:** Detect bypasses by scanning workflow text for floating channels.
    - **Outcome:** Partial
    - **Why:** A text scan cannot see directory-dependent toolchain selection.
  - **Attempt:** On GitHub Actions, assert in a test that `RUSTUP_TOOLCHAIN` starts with the pinned channel, and build from inside the crate directory in shell tests.
    - **Outcome:** Worked
    - **Why:** rustup exports `RUSTUP_TOOLCHAIN` to the processes it launches, so the assertion checks the toolchain actually used, whatever the YAML says.
- **Root cause:** rustup resolves `rust-toolchain.toml` from the current working directory, not from the manifest cargo is asked to build.
- **Resolution:** `ci_runs_on_pinned_toolchain` in `codex-switcher/tests/ci_workflow_policy.rs`; `tests/log_permissions_and_uninstall.sh` builds in a `cd codex-switcher` subshell.
- **Verification:** With `GITHUB_ACTIONS=true`, the test passes inside the crate and fails via `--manifest-path` from another directory.
- **Prevention/follow-up:** Run cargo from inside `codex-switcher/`; AGENTS.md states the rule.
- **Reusable learning:** A directory-scoped toolchain pin needs a runtime check of the toolchain actually used; `--manifest-path` silently bypasses it.
- **References:** `codex-switcher/tests/ci_workflow_policy.rs`, `tests/log_permissions_and_uninstall.sh`, [2026-09-26-floating-rust-toolchain-hid-failing-clippy-gate.md](2026-09-26-floating-rust-toolchain-hid-failing-clippy-gate.md).
