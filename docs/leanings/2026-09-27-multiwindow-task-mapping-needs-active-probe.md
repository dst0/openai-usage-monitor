# 2026-09-27 — Multiwindow task mapping needs an active probe

- **Status:** Partial
- **Task/context:** Investigating preservation of selected ChatGPT tasks across a Desktop restart.
- **Unexpected observation or failure:** The existing window inventory can count exact user windows, while ordinary task deep links navigate the primary window and IPC owner discovery does not expose a verified WindowServer window ID. Counting windows alone cannot recreate each selected task.
- **Evidence:** `codex-window-restore` reconciles WindowServer IDs with Accessibility standard-window frames; `codex_app_lifecycle.rs` blocks shutdown for more than one window. The Desktop's Copy deeplink command is window-focused, so a passive inventory cannot read its result.
- **Approaches tried:**
  - **Attempt:** A mock-only Rust capture/restore backend.
    - **Outcome:** Did not work.
    - **Why:** It would not provide a callable native path or prove Desktop navigation; it was removed before publication.
  - **Attempt:** An explicit native diagnostic using window focus and Copy deeplink.
    - **Outcome:** Partial.
    - **Why:** It can observe distinct links while checking window mapping stability without changing restart safety, but clipboard attribution, per-window navigation, and IPC owner-to-window binding remain unproven.
- **Root cause:** The known Desktop IPC contract addresses a task owner client, while the native inventory identifies WindowServer windows; no verified bridge joins those identities or restores a task into a specific replacement window.
- **Resolution:** Add an opt-in probe that rejects ambiguous geometry, process/window drift, clipboard races observable through change counts, duplicate task IDs, and invalid links. Its helper returns only process/window IDs and a count, while the CLI prints only the observed count. Keep the multiwindow shutdown guard and automatic switching disabled.
- **Verification:** Pure Swift link-validation and frame-mapping tests and Rust service, keymap, and helper-response tests run without live Desktop state; native helper compilation verifies its API. No live UI probe or restoration proof is claimed. Review hardening of the synthesized shortcut is recorded in `2026-09-27-synthesized-desktop-shortcut-needs-keymap-and-focus-proof.md`.
- **Prevention/follow-up:** Prove a window-targeted navigation path and owner-client-to-window identity mapping in the installed app before persisting a snapshot, restoring windows, or lifting the guard. One competing write of a syntactically valid task link cannot be attributed to the source window from a change count alone. A clipboard write between a change-count check and restoration is not atomically preventable, so this diagnostic leaves the copied link rather than overwriting a concurrent clipboard write.
- **Reusable learning:** Window count plus task URLs does not prove exact multiwindow recovery; verify each task's selected window and IPC owner after targeted navigation.
- **References:** `scripts/CodexWindowTaskProbe.swift`, `codex-switcher/src/distribution/window_task_probe_service.rs`, `codex-switcher/src/distribution/system_window_restore_backend.rs`, `codex-switcher/src/switcher/codex_app_lifecycle.rs`, `2026-09-26-window-task-mapping-unavailable.md`.
