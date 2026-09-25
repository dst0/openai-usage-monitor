# Recovery banner hand-off

This module is intentionally self-contained so the recovery coordinator can
wire it without passing session metadata through process arguments.

`RecoveryBannerService::begin` creates one private operation lease and writes
`CODEX_HOME/recovery-runs/restore-banner.json` through a `0600` temporary file
and atomic rename. The payload contains the fixed Russian title and
explanation, the expected Codex PID and kernel birth identity, the saved global
top-down window rectangle and AppKit display rectangle (negative origins
are valid), and every
sanitized row in the form `project / title · short-id`. Full paths and UUID-like
identifiers are reduced before they reach the payload. The status is per row,
so the helper can report pending, in-progress, completed, failed, or skipped.

The native Swift helper must acquire the sibling
`restore-banner.display.lock` with the same-user non-blocking lease before
creating a single `.nonactivatingPanel`. It should read the JSON, verify the
expected PID and birth identity, find the layer-zero Codex window for that
exact PID, and center a `540pt` panel `12pt` below the window's top edge. It
must convert WindowServer coordinates to AppKit coordinates using the owning
display, selecting the display by greatest visible overlap. With window
preservation disabled, a read-only WindowServer capture places the banner
without Accessibility or any window restore. A missing window or banner does
not block credential rotation in that explicit mode. The banner text must not
claim that the prior window position will be restored in this mode. It
must retain all rows in a scrollable/accessibility-readable region and keep
the panel alive for at least `minimum_visible_ms` (5 seconds) before dismissing
or being terminated by its owner.

Minimal coordinator wiring:

1. Capture the exact Codex main PID, kernel birth token, visible window frame,
   and the owning screen before stopping the app.
2. Build `RecoverySession::from_raw` for every target and call `begin` with the
   existing operation ID.
3. Start the helper with only `--payload <absolute-payload-path>`; do not place
   project, title, thread ID, or window metadata in argv.
4. Update the service after each recovery phase with `update_status` or
   `update_sessions`, and hold it until the helper's minimum visibility and the
   final outcome have been logged.
5. Call `finish` on a terminal outcome; `Drop` also removes the private payload
   if a failure unwinds the coordinator.
