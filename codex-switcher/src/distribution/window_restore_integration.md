# Window restore integration contract

The new `WindowRestoreService` is intentionally standalone until the shared recovery coordinator can pass the same operation ID through capture, relaunch, and restore. The minimum wiring is:

1. Add module declarations for each `window_restore_*.rs` file in `distribution/mod.rs` and re-export only `WindowRestoreBackend`, `WindowRestoreService`, the capture/frame/identity models, `RestoreReport`, and `RestoreOutcome`.
2. Implement `WindowRestoreBackend` in the macOS helper boundary. Process inspection must return the exact PID and a stable birth identity from the same process. Window capture must select the main Codex window for that identity and return its frame plus display identity, including negative display origins.
3. Pass the captured `WindowCapture` through the restart coordinator. On restore, call `WindowRestoreService::restore` with the original operation ID and reason. Treat `Restored` as the only success; `Partial` and `Failed` must reach the coordinator outcome and audit log.
4. Implement backend writes in this order: position, size, position. Read the actual frame after the final write. Do not search for or fall back to an unrelated process by application name.
5. Keep helper output behind the backend boundary. The service sanitizes every event detail before it is exposed to the audit logger; the backend must not put secrets in errors.

The service does not enable accessibility-wide enhanced UI behavior and does not launch AppleScript itself. That decision remains in the platform backend so a failed helper, accessibility query, or script is observable as a failed or partial operation.
