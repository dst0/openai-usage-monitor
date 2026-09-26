# 2026-09-26 — Active auth reader followed a replaced path

- **Status:** Resolved
- **Task/context:** Reviewing credential reads used by account setup and direct switching.
- **Unexpected observation or failure:** `read_active_auth_json` followed an `auth.json` symlink and could read an inode that was replaced while reading.
- **Evidence:** An isolated synthetic symlink test returned credentials through a symlink under the old `File::open` implementation. A deterministic after-open replacement test exercises the pathname identity check; a permissions test rejects world-readable auth.
- **Approaches tried:**
  - **Attempt:** Check pathname existence before ordinary `File::open`.
    - **Outcome:** Did not work.
    - **Why:** Existence is a separate race and ordinary open follows symlinks.
  - **Attempt:** Open with `O_NOFOLLOW`, require a private regular descriptor, then compare the final named inode to the opened descriptor.
    - **Outcome:** Worked in focused tests.
    - **Why:** A symlink is rejected at open and a replaced pathname is detected after the read.
- **Root cause:** Path-based prechecks were treated as if they constrained the later file descriptor.
- **Resolution:** The active-auth reader now validates the descriptor and final pathname identity with sanitized errors.
- **Verification:** `cargo test storage::active_auth_read_tests:: --bin codex-mon` passed all three isolated tests. Live Desktop behavior was not exercised for this reader change.
- **Prevention/follow-up:** Use descriptor-based no-follow reads for credentials and check exact inode identity when later operations depend on the named path.
- **Reusable learning:** A pathname existence check does not make a later credential-file open safe.
- **References:** `codex-switcher/src/storage.rs`, `codex-switcher/src/storage/active_auth_read.test.rs`, `CODEX.md`.
