# 2026-09-20 — Copied CLI requires a fresh ad-hoc signature

- **Status:** Resolved
- **Task/context:** Repair the macOS CLI installation path in `scripts/install.sh` without running the installer or changing the installed CLI.
- **Unexpected observation or failure:** A `codex-mon` binary copied to a fresh staging inode could pass `codesign --verify --strict` on disk and still be killed by macOS with `SIGKILL (Code Signature Invalid)` when launched.
- **Evidence:** Live testing isolated outside this change showed the copied staging binary failed to launch, while forcibly applying a fresh ad-hoc signature to an isolated copy made `--version` launch successfully.
- **Approaches tried:**
  - **Attempt:** Verify the copied binary first and ad-hoc sign only when strict verification fails.
    - **Outcome:** Did not work
    - **Why:** Strict on-disk verification could succeed even though macOS rejected the copied binary at execution time, so the conditional skipped the required signature refresh.
  - **Attempt:** Always apply a fresh ad-hoc signature after copy, permissions, and extended-attribute cleanup.
    - **Outcome:** Worked
    - **Why:** It replaced the copied signature state before strict verification and the launch smoke test.
- **Root cause:** The installer treated successful strict on-disk verification of the copied artifact as proof that its signature would be accepted at execution time. The observed kernel launch result disproved that assumption.
- **Resolution:** The staging flow now unconditionally runs `codesign --sign - --force` before strict verification and `--version`, while retaining the fresh inode and atomic pathname replacement.
- **Verification:** `tests/install_cli_staging_signature.sh` checks the exact non-destructive staging order and prevents conditional signing from returning; `bash -n scripts/install.sh` checks shell syntax. The installer was intentionally not run for this scoped change.
- **Prevention/follow-up:** Keep execution smoke testing after signing and verification. Do not optimize away the unconditional staging signature based only on `codesign --verify` success.
- **Reusable learning:** For a copied macOS executable, refresh the staging inode's ad-hoc signature unconditionally before strict verification and execution smoke testing; on-disk verification alone is not launch proof.
- **References:** `scripts/install.sh`; `tests/install_cli_staging_signature.sh`
