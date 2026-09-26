# 2026-09-26 — Foreground rollout rewrite before dispatch

- **Status:** Resolved
- **Task/context:** Revalidate a restart checkpoint after Desktop owner discovery and before IPC dispatch.
- **Unexpected observation or failure:** Rewriting already scanned bytes on the same inode to add `task_started`, then appending a metadata event, left the append observer unaware of the start. The pre-dispatch check allowed a second turn request.
- **Evidence:** `owner_revalidation_rejects_middle_rewrite_followed_by_append` returned `Ok(true)` before the fix and failed its assertion.
- **Approaches tried:**
  - **Attempt:** Trust inode and increasing length as an append-only guarantee.
    - **Outcome:** Did not work.
    - **Why:** Those properties do not prove previously scanned bytes are unchanged.
  - **Attempt:** Pin pre-dispatch scans to a stable metadata snapshot and retain the checkpoint on later growth or metadata change.
    - **Outcome:** Worked.
    - **Why:** Dispatch cannot proceed with unverified old bytes; a later attempt scans them afresh.
- **Root cause:** The observer checked modification time only when the file length stayed equal.
- **Resolution:** Track whether bytes were scanned and require the same file snapshot before subsequent pre-dispatch reads; reject mutation during those reads. Post-dispatch observation still accepts normal appends.
- **Verification:** The regression and all 96 `recovery::` tests passed.
- **Prevention/follow-up:** A fresh attempt may be needed if Desktop appends while mounting; do not retry an uncertain IPC send.
- **Reusable learning:** An append after an earlier scan cannot certify that the already scanned interval stayed unchanged.
- **References:** `codex-switcher/src/recovery/observer.rs`, `codex-switcher/src/recovery/recovery_target.rs`, `codex-switcher/src/recovery/target_dispatch.test.rs`.
