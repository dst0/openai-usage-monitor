# 2026-09-26 — Credential staging symlink

- **Status:** Resolved
- **Task/context:** Review private, atomic writes for Monitor-owned registry and shared Desktop credentials.
- **Unexpected observation or failure:** The predictable temporary pathname was opened with `create(true)`, which followed a pre-existing symlink and rewrote an unrelated file before the rename.
- **Evidence:** `registry_write_does_not_follow_a_preexisting_temporary_symlink` and `active_auth_write_does_not_follow_a_preexisting_temporary_symlink` each failed before the fix using only isolated temporary files.
- **Approaches tried:**
  - **Attempt:** Reuse a PID-named temporary file under the advisory Monitor lock.
    - **Outcome:** Did not work
    - **Why:** The lock only coordinates Monitor processes; it does not stop another same-user actor from placing a symlink.
  - **Attempt:** Use a random staging name with `create_new`, `O_NOFOLLOW`, and mode `0600`.
    - **Outcome:** Worked
    - **Why:** The writer creates a new regular file and refuses a path that already exists.
- **Root cause:** `create(true)` reused and followed a predictable staging path.
- **Resolution:** Both writers use random, no-follow, exclusive-create temporary files; they clean up only a file they created and sync it before atomic replacement.
- **Verification:** Both regression tests passed after the fix. No live credential files were used.
- **Prevention/follow-up:** Keep the symlink regressions and use exclusive, private staging for every credential file replacement.
- **Reusable learning:** A lock and atomic rename do not make an unsafe temporary-file open safe.
- **References:** `codex-switcher/src/storage.rs`, `codex-switcher/src/storage/accounts_registry_transaction_service.rs`, `codex-switcher/src/storage/accounts_registry_transaction_service.test.rs`.
