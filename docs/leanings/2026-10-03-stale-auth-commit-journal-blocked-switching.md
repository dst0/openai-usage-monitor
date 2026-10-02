# 2026-10-03 — Stale auth-commit journal blocked account switching

- **Status:** Partial
- **Task/context:** Diagnose repeated automatic and manual distribution failures after an interrupted account switch.
- **Unexpected observation or failure:** A private distribution journal remained in `auth_commit_cli` from 2026-09-28. Every later distribution stopped at the journal gate. Retained logs show 16,563 `stale_uncertain_distribution_journal` failures; the original failure record is no longer available.
- **Evidence:** The phase is recorded immediately before a shared-auth write, so it does not prove whether that write happened. Under the recovery operation lock, the current auth token object uniquely matched the active saved account, the journal target was different, the Desktop marker named the active account but its process was dead, no ChatGPT main process was running, and neither a direct-switch journal nor a recovery manifest existed. A bundled Codex CLI process was active, so absence of every possible credential writer was not established.
- **Approaches tried:**
  - **Attempt:** Delete the journal because its worker PID was dead.
    - **Outcome:** Did not use.
    - **Why:** A dead worker is not evidence that an auth write rolled back.
  - **Attempt:** Reconcile current auth, registry, marker, process, and recovery state under the operation lock, then archive and clear only the unchanged journal.
    - **Outcome:** Worked for this one-time state repair.
    - **Why:** The observed active account had superseded the journal target, and the archive preserved the original intent for audit. A fake-state run exercised the same check and clear sequence first.
- **Root cause:** The retained uncertain journal caused all later gate failures. The original reason the journal remained at auth commit cannot be reconstructed from available logs.
- **Resolution:** Saved the unchanged journal as a private, verified Brotli Q6 archive, durably removed that exact journal under the operation lock, and left automatic switching disabled. No credentials or Desktop sessions were changed.
- **Verification:** Readback confirmed the journal was absent, the archive was mode `0600` and decompressed to the source bytes, and `auto_switch_enabled` remained false. This does not prove that a future distribution or cold-task recovery will succeed.
- **Prevention/follow-up:** Keep the fail-closed journal gate. When this code recurs, compare live auth and unique token ownership, registry active ID, Desktop marker and exact process, and recovery checkpoint before an explicit repair; never clear by elapsed time. The operator should inspect the original failure before logs rotate when possible.
- **Reusable learning:** An auth-commit phase records intent before the write; age and dead PID alone cannot establish its outcome.
- **References:** `codex-switcher/src/distribution/distribution_journal_gate_service.rs`, `codex-switcher/src/distribution/distribution_offline_commit_service.rs`, `CODEX.md`.
