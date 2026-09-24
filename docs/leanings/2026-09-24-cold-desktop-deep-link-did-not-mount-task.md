# 2026-09-24 — Cold Desktop deep link did not mount a task

- **Status:** Partial
- **Task/context:** Verify task recovery after the first live automatic ChatGPT Desktop account switch with window preservation disabled.
- **Unexpected observation or failure:** The credential switch and Desktop relaunch succeeded, but two of three eligible tasks did not acquire a Desktop owner during automatic recovery.
- **Evidence:** The daemon reported `PartialSuccess`, with one verified recovery and two `no-client-found` failures. The macOS `open` command accepted each `codex://threads/<id>` request, yet subsequent `thread-owner-discovery` still returned `no-client-found`. Desktop IPC preflight remained healthy. ChatGPT's own task navigation mounted one affected task, after which owner discovery succeeded.
- **Approaches tried:**
  - **Attempt:** Open cold tasks through macOS deep links, including explicit application targeting.
    - **Outcome:** Did not work.
    - **Why:** A successful `open` exit did not cause the installed Desktop build to mount those tasks.
  - **Attempt:** Navigate to one affected task using ChatGPT's task navigation, inspect its turn, then use `cxi resume <id>`.
    - **Outcome:** Worked.
    - **Why:** Owner discovery succeeded after native navigation; recovery then produced the expected turn, substantive work, and verification soak. The other affected task had already completed and was not resumed again.
- **Root cause:** The current Desktop build did not mount these cold tasks from external deep links. The reason for that application behavior is unconfirmed; the switcher cannot treat an accepted `open` exit as proof of ownership.
- **Resolution:** Keep owner discovery as the gate before dispatch. Report partial recovery and direct operators to open an affected task in ChatGPT, inspect its current turn, and run `cxi resume <id>` only if still interrupted. The automatic cold-task mount remains unresolved.
- **Verification:** One task recovered automatically; one affected task recovered after native navigation and explicit resume; the other affected task was observed completed. No second resume was sent to the completed task. Documentation now states the limit and the safe recovery path.
- **Prevention/follow-up:** Investigate a supported Desktop-owned navigation route usable by the daemon, then add a regression that proves a cold task becomes owned before dispatch. Until then, preserve the partial outcome and avoid duplicate turns.
- **Reusable learning:** External deep-link acceptance is not evidence that a Desktop task has mounted; verify owner discovery and the resulting turn.
- **References:** `README.md`, `CODEX.md`, `AGENTS.md`, `codex-switcher/src/recovery/desktop_ipc.rs`, `codex-switcher/src/distribution/distribution_recovery_audit_service.rs`, `2026-09-24-launchd-accessibility-blocked-auto-switch.md`.
