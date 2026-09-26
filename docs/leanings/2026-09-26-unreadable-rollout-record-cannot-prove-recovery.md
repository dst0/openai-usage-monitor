# 2026-09-26 — Unreadable rollout records cannot prove recovery

- **Status:** Resolved
- **Task/context:** Cold-task checkpoint replacement and owner-routed foreground recovery after a Desktop account switch.
- **Unexpected observation or failure:** A newline-terminated JSONL record larger than the bounded line buffer was silently skipped. Malformed JSON or a JSON object missing required event fields was also ignored. A later clean record could make a failed or already-started turn appear safe to dispatch or verified.
- **Evidence:** New regression tests reproduced checkpoint replacement after an oversized failed `task_complete`, duplicate-dispatch eligibility after an oversized `task_started`, and false success after an oversized failure following agent work. The three tests failed before the guard and passed afterward.
- **Approaches tried:**
  - **Attempt:** Treat only an unfinished oversized line as incomplete.
    - **Outcome:** Did not work.
    - **Why:** The transient `oversized` flag cleared at newline before either foreground or confirmation checked it.
  - **Attempt:** Retain sticky flags for oversized and malformed completed records, reject structurally incomplete event objects, and reject their interval as recovery proof.
    - **Outcome:** Worked for the tested paths.
    - **Why:** A skipped record could contain any lifecycle event, including a turn start or error.
- **Root cause:** The parser discarded unreadable records while continuing to trust other lifecycle evidence from the same post-checkpoint interval.
- **Resolution:** The bounded observer records unreadable lines persistently. Foreground dispatch and verification fail closed; cached evidence and fresh confirmation cannot authorize destructive retry changes.
- **Verification:** `cargo test --bin codex-mon recovery::`; specific regressions in `observer.test.rs`, `target_dispatch.test.rs`, and `manifest_store.test.rs`.
- **Prevention/follow-up:** Keep large-volume scan tests made of many individually valid bounded records. A future streaming parser may safely recover useful evidence from larger records, but it must prove all lifecycle fields without unbounded memory.
- **Reusable learning:** Never infer an error-free turn across a skipped rollout record.
- **References:** `codex-switcher/src/recovery/observer.rs`, `codex-switcher/src/recovery/checkpoint_confirmation.rs`, `codex-switcher/src/recovery/recovery_target.rs`.
