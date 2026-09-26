# 2026-09-26 — Distribution journal paths needed private bounded I/O

- **Status:** Resolved
- **Task/context:** Hardening the account-distribution recovery journal before the next installed-app test.
- **Unexpected observation or failure:** The writer used a predictable PID-only temporary name with truncating open, so a same-user symlink at that name redirected the write to another file. The reader followed symlinks and accepted a valid oversized journal or a world-readable one.
- **Evidence:** Four isolated synthetic regressions failed against the old code: a staging symlink changed a file containing only test data, a final-path symlink was accepted, a symlinked journal loaded, and an oversized valid journal loaded. All passed after the change; no live credentials were used.
- **Approaches tried:**
  - **Attempt:** Keep PID-only staging and rely on atomic rename.
    - **Outcome:** Did not work.
    - **Why:** Truncation followed the staging symlink before rename.
  - **Attempt:** Use random same-filesystem `create_new` staging with `O_NOFOLLOW`, check the existing final path, and bound private no-follow reads.
    - **Outcome:** Worked in focused synthetic tests.
    - **Why:** A pre-planted staging path cannot be reused, unsafe final paths fail before replacement, and reads stop at the fixed document limit.
- **Root cause:** Journal I/O lacked the same path, permission, and size checks used for credential-adjacent state.
- **Resolution:** Journal reads now require a regular same-user 0600 file with a stable named inode and at most 16 KiB. Writes stage through a random exclusive 0600 file, verify the staging and final path identities, atomically replace the journal, and sync the directory. Clear rejects an unsafe final path.
- **Verification:** Five focused journal tests pass after four red regressions; `cargo check` and file-limit checks remain required at integration. The official Desktop does not participate in Monitor locks, so the final pathname check does not prove the absence of every possible uncooperative same-user race.
- **Prevention/follow-up:** Apply bounded no-follow reads and exclusive unpredictable staging to small recovery journals; keep valid historical fixtures at mode 0600.
- **Reusable learning:** A rename cannot make an earlier symlink-following truncation safe.
- **References:** `codex-switcher/src/distribution/distribution_journal.rs`, `codex-switcher/src/distribution/distribution_journal.test.rs`, `codex-switcher/src/distribution/distribution.test.rs`.
