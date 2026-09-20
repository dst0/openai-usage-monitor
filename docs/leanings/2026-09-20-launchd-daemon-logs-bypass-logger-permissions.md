# 2026-09-20 — launchd daemon logs bypass logger permissions

- **Status:** Resolved
- **Task/context:** Private log storage for the Codex Monitor daemon and its uninstall contract.
- **Unexpected observation or failure:** The Rust logger opened its own active log with mode `0600`, while launchd-created daemon stdout and stderr files remained group/world-readable until the daemon explicitly repaired them.
- **Evidence:** A live permissions inspection found the two daemon stream files at `0644`; the Rust logger's directory and active switcher log path already enforced private modes.
- **Approaches tried:**
  - **Attempt:** Rely on the logger's `OpenOptions` mode for all Monitor logs.
    - **Outcome:** Did not work.
    - **Why:** launchd opens inherited stdout/stderr before Rust code runs, so those files never pass through the logger.
  - **Attempt:** Enforce modes at daemon startup and during installation, rejecting symlinked paths.
    - **Outcome:** Worked.
    - **Why:** Both the runtime and pre-existing installation state are covered without changing log content or rotation.
- **Root cause:** launchd owns creation of daemon stdout/stderr, outside the logger's file-open path.
- **Resolution:** Added fd-anchored, no-follow Rust services for permission setup, logger append/rotation, cancellation-marker creation, and Monitor log cleanup. The installer calls the hidden helper after building the CLI and before configuring launchd; the uninstaller calls the same helper before removing the CLI. Cleanup removes only regular Monitor files, exact numeric timestamp archives, temporary state files, and the Monitor-owned recovery tree; it preserves foreign archives and refuses symlinked or non-regular account registries.
- **Verification:** Focused Rust runs passed five permission tests and five logger tests, including symlinked active/archive write and rotation rejection and malformed timestamp retention. The isolated shell test passed installer mode repair and missing-path creation, exact dry-run inventory including temporary-file cleanup, malformed/foreign archive preservation, official-state content/mode preservation, normal versus explicit account purge, symlink refusal for log/recovery/CODEX_HOME/accounts paths, and stubbed LaunchServices/launchd commands. The full Rust gate still has the repository's pre-existing file-limit failures outside this change.
- **Prevention/follow-up:** Keep active streams uncompressed; use the fd-anchored helper before daemon launch and during uninstall so path replacement cannot redirect Monitor log mutations. Keep the helper's archive matcher aligned with logger's `YYYYMMDD-HHMMSS` naming format.
- **Reusable learning:** Logger-level permissions do not protect inherited process streams; installation and uninstall must share one descriptor-anchored ownership boundary, and archive cleanup must match the producer's exact filename grammar.
- **References:** `codex-switcher/src/distribution/monitor_log_io_service.rs`; `codex-switcher/src/distribution/monitor_log_cleanup_service.rs`; `tests/log_permissions_and_uninstall.sh`.
