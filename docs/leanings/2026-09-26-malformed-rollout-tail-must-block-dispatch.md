# 2026-09-26 — A malformed terminal rollout record could authorize recovery

- **Status:** Resolved
- **Task/context:** Reviewing automatic restart recovery after repeated banner-without-resume reports.
- **Unexpected observation or failure:** Tail classification skipped every malformed JSONL record. A truncated terminal `task_complete` could be ignored, leaving an older `task_started` classified as active and eligible for unattended dispatch.
- **Evidence:** A synthetic test with a valid start followed by an incomplete completion returned `ActiveInProgress` before the fix, instead of `Unknown`. No live user transcript was copied into the test.
- **Approaches tried:**
  - **Attempt:** Skip all malformed records as possible seek fragments.
    - **Outcome:** Did not work.
    - **Why:** Only the first record after a bounded seek may be a fragment; a malformed newer record can change turn lifecycle.
  - **Attempt:** Drop only that first seek fragment, pin reads to the captured length, require a final newline, and classify any later malformed record as unknown.
    - **Outcome:** Worked in focused synthetic tests.
    - **Why:** An incomplete terminal event cannot be mistaken for a still-active turn, and file growth cannot exceed the read limit.
- **Root cause:** The parser applied a narrow seek-window exception to every line in the tail and read beyond its initial length snapshot when the file grew.
- **Resolution:** Discovery tail reads are bounded to the captured length. The classifier fails closed on a malformed newer line or incomplete final record.
- **Verification:** The malformed-terminal regression was red before the change; focused green tests and the final Rust gate are recorded in the associated PR.
- **Prevention/follow-up:** Keep discovery classification conservative and retain explicit user recovery for unknown states.
- **Reusable learning:** A partial-record exception must be tied to the one boundary that can actually produce it.
- **References:** `codex-switcher/src/switcher/thread_rollout_inspector.rs`, `codex-switcher/src/switcher.test.rs`.
