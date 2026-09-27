# 2026-09-28 — macOS 14 ships no lockf(1), so uninstall could not probe any lock

- **Status:** Resolved
- **Task/context:** PR #30 made uninstall remove a killed install's leftovers only after probing the install lock. It also runs `tests/log_permissions_and_uninstall.sh` in the required Rust CI job on the `macos-14` runner. See [2026-09-28-uninstall-kept-killed-install-staging.md](2026-09-28-uninstall-kept-killed-install-staging.md).
- **Unexpected observation or failure:**
  - After the review asked for the held-lock scenario to use `install.sh`'s own `acquire_install_lock`, CI failed with `dry-run did not preserve ... (an installer holds the install lock)`, although every local run passed.
  - The installer printed no "Another installation" message, so its lock call had returned quietly.
- **Evidence:**
  - A failure message extended with probe diagnostics reported: `plan: preserve ... (the install lock cannot be verified); lockf on PATH: none; /usr/bin/lockf probe status: 127; macOS 14.8.9`.
  - Apple's `shell_cmds` source matches: the `lockf` fd form (`lockf [-s] [-t seconds] fd`) first appears in `shell_cmds-319`, and the stock macOS 14 image has no `lockf` binary at all. The older source (`shell_cmds-309`) would reject the fd form with `EX_USAGE`, but no shipped release with that binary was found; a third-party report says `lockf` first shipped with macOS 15.
  - `acquire_install_lock` found no `lockf` and took its `python3` `fcntl.flock` fallback, which the installer can rely on because it already requires the Command Line Tools.
  - A local check showed that perl's `flock`, Python's `fcntl.flock`, `lockf`'s fd form, and `lockf`'s `O_EXLOCK` file form all contend with each other: each probe returned 75 while another held the lock.
- **Approaches tried:**
  - **Attempt:** Keep `/usr/bin/lockf` as the only probe.
    - **Outcome:** Did not work.
    - **Why:** It failed closed correctly. But on macOS 14 the install lock file always exists after an install, so uninstall could never remove a killed install's leftovers.
    - **Pre-existing bug:** it also exposed that `remove_unlocked_file` had always kept `daemon.lock`, `codex.lock`, `monitor.lock`, `desktop-recovery.lock`, and the install lock on macOS 14, so every uninstall there ended with warnings.
  - **Attempt:** Fall back to `python3`, as the installer does.
    - **Outcome:** Rejected.
    - **Why:** Without the Command Line Tools, `/usr/bin/python3` is a stub that opens an install dialog. The Menu Bar app runs the uninstaller in the background, where that dialog would be unexpected.
  - **Attempt:** Probe with `/usr/bin/lockf` when present, otherwise with `/usr/bin/perl -MFcntl=:flock` (`LOCK_EX | LOCK_NB`, exit 75 on `EWOULDBLOCK`, optional unlink while holding the lock). Any other result means unverified.
    - **Outcome:** Worked.
    - **Why:** `/usr/bin/perl` is present on the macOS 14.8.9 runner (the CI run of this change passes through it) and on macOS 27.2. Its `flock` is BSD `flock(2)`, the same lock the installer takes.
- **Root cause:** The design assumed `lockf(1)` exists on every supported macOS, because it exists on the development host (macOS 27). The minimum target is macOS 13, and macOS 14 has no `lockf`. Which release first ships it was not checked; the fd form appears in `shell_cmds-319`.
- **Resolution:** `try_flock` in `scripts/uninstall.sh` serves both the install-lock probe and `remove_unlocked_file`. The latter now distinguishes a held lock from an unknown state. As with `lockf`, a failed unlink after a free probe is left to the caller's `remove_path`, which reports it.
- **Verification:** The shell test now runs:
  - the held-lock checks, dry run and confirmed run, through both the host's probe and a copy with no `lockf` (the perl path);
  - a copy with neither probe (unverified; the lock file is kept);
  - a free lock removed through the perl path.

  A local emulation (no `lockf` in the uninstaller, `python3` forced in `acquire_install_lock`) passes, and mutations of the perl probe are caught.
- **Prevention/follow-up:**
  - `AGENTS.md` and `CODEX.md` record which probe each macOS release uses.
  - Failure messages in the lock scenarios print the probe environment, so a runner difference shows its cause at once.
- **Reusable learning:**
  - Check a system tool's availability on the oldest supported OS, not only on the development host.
  - When a check depends on a tool, make the test that runs in CI exercise the production code path, such as the installer's own lock function.
  - Write failure messages that report what the environment provided.
- **References:** `scripts/uninstall.sh` (`try_flock`, `install_lock_blocker`, `remove_unlocked_file`), `scripts/install.sh` (`acquire_install_lock`), `tests/log_permissions_and_uninstall.sh`, apple-oss-distributions/shell_cmds `lockf/lockf.c` at `shell_cmds-309` and `shell_cmds-319.0.1`.
