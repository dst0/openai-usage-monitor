# 2026-09-25 — Rebased recovery code exposed a Clippy lint

- **Status:** Resolved
- **Task/context:** Rebase the log redaction parity PR onto the newly merged cold-task recovery change.
- **Unexpected observation or failure:** Local `cargo clippy --offline -- -D warnings` failed after the rebase, although the log redaction patch did not modify recovery code.
- **Evidence:** Rust 1.94.1 flagged `recover_threads(&[id.clone()], mode)` in `deferred_recovery_service.rs` with `cloned_ref_to_slice_refs`. The new main branch contained this line.
- **Approaches tried:**
  - **Attempt:** Rebase the parity commit onto the new main and run the full lint gate.
    - **Outcome:** Found a lint in the combined tree.
    - **Why:** The rebase changed the code included in the PR's head revision.
  - **Attempt:** Replace the needless clone with `std::slice::from_ref(&id)`.
    - **Outcome:** Worked.
    - **Why:** The callee receives a borrowed one-element slice with the same value and lifetime for this call.
- **Root cause:** The newly merged recovery code used a cloned string where a borrowed slice suffices; the local compiler enforces this Clippy lint under `-D warnings`.
- **Resolution:** Pass `std::slice::from_ref(&id)` to `recover_threads`.
- **Verification:** Local `cargo clippy --offline -- -D warnings` passes on the rebased tree; the full test suite and remote CI are still required.
- **Prevention/follow-up:** Rerun lint and tests on the exact rebased PR head, even when the parity patch applies without conflicts.
- **Reusable learning:** A clean rebase does not prove the combined head passes its validation gates.
- **References:** `codex-switcher/src/recovery/deferred_recovery_service.rs`, OpenAI Usage Monitor PR #7 and PR #8.
