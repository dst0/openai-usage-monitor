# 2026-09-26 — Status cache staging must reject predictable symlinks

- **Status:** Resolved
- **Task/context:** Inspect status-cache writes while closing the switch-setting race.
- **Unexpected observation or failure:** A preexisting symlink at the PID-derived temporary status filename redirected a write into a synthetic unrelated file.
- **Evidence:** The focused symlink regression changed a sentinel before the fix and preserved it after the fix.
- **Approaches tried:**
  - **Attempt:** Reopen a predictable temporary filename with an ordinary write.
    - **Outcome:** Did not work.
    - **Why:** The path could already be a symlink.
  - **Attempt:** Create a random temporary file with `create_new`, `O_NOFOLLOW`, and mode `0600`, then rename it.
    - **Outcome:** Worked.
    - **Why:** The write cannot follow the old path and the replacement remains atomic.
- **Root cause:** The status-cache staging path assumed PID names were unique and owned.
- **Resolution:** Status staging uses an unpredictable exclusive no-follow temporary file.
- **Verification:** `status_writer_never_follows_a_preexisting_temporary_symlink` passed after failing against the old writer.
- **Prevention/follow-up:** Apply the same staging rule to all monitor-owned cache and credential writes.
- **Reusable learning:** A predictable temporary filename is unsafe even for a derived cache.
- **References:** `codex-switcher/src/storage/status_file_service.rs`, `codex-switcher/src/storage/status_file_service.test.rs`, `CODEX.md`.
