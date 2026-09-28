# 2026-09-28 — macOS 13's `mktemp -t` appends eight characters, not ten

- **Status:** Resolved
- **Task/context:** A late review of PR #30 said that the uninstaller's remote-clone pattern (`codex-mon-install-XXXXXX.` followed by exactly ten `mktemp` characters) misses clones made on macOS 13. The earlier learning [2026-09-28-uninstall-kept-killed-install-staging.md](2026-09-28-uninstall-kept-killed-install-staging.md) measured ten characters on the development host and generalized that to macOS.
- **Unexpected observation or failure:** On macOS 13, `mktemp -d -t codex-mon-install-XXXXXX` creates `codex-mon-install-XXXXXX.` plus eight characters. The uninstaller never listed such a clone, so a killed remote install on macOS 13 left its clone behind for good.
- **Evidence:**
  - `mktemp/mktemp.c` at `shell_cmds-278` (macOS 13.0) and `279.120.2` (macOS 13.5) builds the `-t` template as `"%s/%s.XXXXXXXX"` (eight). From `302.0.1` (macOS 14.0) on, it is `"%s/%s.XXXXXXXXXX"` (ten). Release mapping: apple-oss-distributions/distribution-macOS tags `macos-130`, `macos-135`, and `macos-140`.
  - Every version uses `_CS_DARWIN_USER_TEMP_DIR` before `TMPDIR` for `-t`, as the earlier learning found.
- **Approaches tried:**
  - **Attempt:** Accept exactly eight or exactly ten characters: `(${MKTEMP_CHAR}{8}|${MKTEMP_CHAR}{10})`.
    - **Outcome:** Worked.
    - **Why:** It matches both formats and still rejects 7, 9, and 11 characters, which the look-alike fixtures cover. Bash 3.2's `=~` uses the system regex, which supports interval expressions inside alternation.
- **Root cause:** The format was measured on one macOS release and assumed for the oldest supported one, whose `mktemp` source differs.
- **Resolution:** `REMOTE_CLONE_NAME` in `scripts/uninstall.sh`. `README.md`, `CODEX.md`, and `AGENTS.md` name both lengths.
- **Verification:** `tests/log_permissions_and_uninstall.sh` removes a valid 8-character clone (`m13Cln8x`) and keeps 7-, 9-, and 11-character look-alikes. A mutant that accepts only ten characters fails the test.
- **Prevention/follow-up:** None beyond the fixture.
- **Reusable learning:** Before matching the names a system tool generates, read that tool's source for every supported OS release, not only the release on the development host.
- **References:** `scripts/uninstall.sh` (`REMOTE_CLONE_NAME`), `tests/log_permissions_and_uninstall.sh`, apple-oss-distributions/shell_cmds `mktemp/mktemp.c` at `shell_cmds-278`, `279.120.2`, and `302.0.1`.
