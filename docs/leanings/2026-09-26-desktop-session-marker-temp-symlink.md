# 2026-09-26 — Desktop session marker staging followed a predictable symlink

- **Status:** Resolved
- **Task/context:** Auditing credential safety during account-switch relaunch.
- **Unexpected observation or failure:** The Desktop session marker used a PID-only temporary filename opened with truncation. A symlink at that filename redirected the write to another same-user file before the marker was renamed.
- **Evidence:** A synthetic regression placed a symlink at the former staging path. The old `save` changed a test victim file; no real credential was used. The regression passed after changing the writer.
- **Approaches tried:**
  - **Attempt:** Rely on the later atomic rename.
    - **Outcome:** Did not work.
    - **Why:** Truncation had already followed the staging symlink.
  - **Attempt:** Use random exclusive no-follow staging and reject unsafe marker paths on read and write.
    - **Outcome:** Worked in focused tests.
    - **Why:** A planted staging path cannot be reused, and marker reads are bounded to a private stable inode.
- **Root cause:** The credential-adjacent marker did not use the protected file pattern used for other Monitor state.
- **Resolution:** Marker staging is now random, exclusive, `0600`, and no-follow, followed by checked atomic replacement and directory sync. Reads require a same-user regular `0600` file of at most 16 KiB with a stable named inode. Safety-critical callers fail closed on malformed or unsafe markers.
- **Verification:** The symlink-victim regression was red before the fix; focused green test and final Rust gate are recorded in the associated PR. A same-user process that deliberately races an inode check remains outside a cooperative Monitor lock.
- **Prevention/follow-up:** Apply protected staging and checked reads to every credential-adjacent state file, including small session markers.
- **Reusable learning:** Atomic rename does not protect a file opened unsafely before that rename.
- **References:** `codex-switcher/src/distribution/desktop_app_session.rs`, `codex-switcher/src/distribution/desktop_app_session.test.rs`.
