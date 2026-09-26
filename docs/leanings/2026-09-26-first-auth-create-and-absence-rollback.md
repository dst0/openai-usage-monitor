# 2026-09-26 — First auth creation and rollback must preserve absence

- **Status:** Resolved
- **Task/context:** Direct switching when the account registry exists but `auth.json` did not exist before the switch.
- **Unexpected observation or failure:** A new auth file could overwrite another process's file created during the switch, and a later registry failure restored a tokenless file instead of the original absence.
- **Evidence:** A synthetic interleaving test created an external auth file just before first-write commit; the old writer replaced it. Another test showed a tokenless file after rollback. Both failed before the fix and passed after it. A staged-file race and external replacement during rollback were also covered.
- **Approaches tried:**
  - **Attempt:** Use the ordinary atomic rename writer and synthesize an empty prior auth document.
    - **Outcome:** Did not work.
    - **Why:** Rename overwrites a newly appeared path, and the synthetic document was not the original filesystem state.
  - **Attempt:** Stage a private file on the same filesystem, publish it with a no-clobber hard link, and conditionally remove the exact committed file on failure.
    - **Outcome:** Worked in focused synthetic tests.
    - **Why:** Link creation fails if another file appeared; rollback refuses changed content or inode and verifies absence afterward.
- **Root cause:** Missing-file state was represented as a tokenless `AuthJson`, and the regular replacement writer could not enforce create-if-absent.
- **Resolution:** First auth creation is no-clobber; rollback uses a guarded compare-and-remove that rejects an observed content or inode change. The official Desktop does not take the Monitor lock, so a final pathname check cannot eliminate every possible same-user race after that check.
- **Verification:** Focused first-auth creation and rollback tests passed in isolated `CODEX_HOME`; no live auth was changed.
- **Prevention/follow-up:** Represent absence explicitly in transaction state, and test both a file appearing before publish and a file changing before rollback.
- **Reusable learning:** Roll back a newly created credential file to absence, never to a fabricated empty credential document.
- **References:** `codex-switcher/src/storage/active_auth_create_service.rs`, `codex-switcher/src/storage/active_auth_remove_service.rs`, `codex-switcher/src/switcher/codex_availability_service.test.rs`, `CODEX.md`.
