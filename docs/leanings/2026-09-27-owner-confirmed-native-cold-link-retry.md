# 2026-09-27 — Native cold-link retry still needs Desktop owner confirmation

- **Status:** Partial
- **Task/context:** Reconsider the [earlier rejected native URL experiment](2026-09-27-background-cold-link-may-steal-focus.md) after the device owner clarified that ChatGPT may come to the foreground during recovery.
- **Unexpected observation or failure:** One cold quota task mounted through a pinned native LaunchServices URL after ordinary `open -g -a` left it ownerless for over 15 seconds. In another check, the native call succeeded but the task was ownerless immediately afterward; it acquired an owner later, so the immediate check did not establish permanent failure.
- **Evidence:** The installed Desktop IPC `thread-owner-discovery` result changed only after navigation, and no follower IPC was sent during either check. The installed ChatGPT 26.924.20706 deep-link handler makes its primary window visible before task navigation. Foreground activation is permitted by the owner. No task payload, credential, or account identity is recorded.
- **Approaches tried:**
  - **Attempt:** Trust URL launch exit status as a successful mount.
    - **Outcome:** Did not work.
    - **Why:** URL delivery can precede or fail to produce a Desktop owner.
  - **Attempt:** Retain ordinary URL delivery and add a bounded native attempt to the exact ChatGPT bundle after ownerless waiting.
    - **Outcome:** Partial.
    - **Why:** It mounted one cold quota task in a live A/B check, while another check showed that owner acquisition can lag launch acknowledgement. Neither proves a new recovery turn.
- **Root cause:** The Desktop mounts task ownership asynchronously after accepting a URL; acceptance and owner discovery are separate steps. Why some ordinary deliveries remain ownerless is still unknown.
- **Resolution:** Keep the ordinary URL path and one pinned native fallback for ownerless tasks. Continue polling for a real Desktop owner and preserve the checkpoint if none appears. Send follower IPC only to a verified owner with the existing account, process, queue, and rollout checks.
- **Verification:** Live read-only owner probes demonstrated one native mount and delayed ownership in a second trial. Focused Rust tests cover retry timing, error continuation, and deferred attempt bookkeeping. A full quota-interrupted recovery turn after account switch and exact multiwindow selected-task restoration remain unverified.
- **Prevention/follow-up:** Test the installed recovery path through URL delivery, owner discovery, one IPC dispatch, exact turn ID, substantive post-checkpoint work, banner, and multiwindow selection before enabling automatic switching. Apple's `LSOpenFromURLSpec` API is deprecated, so recheck this bounded fallback after macOS updates.
- **Reusable learning:** Treat URL delivery as a navigation request; wait for Desktop owner proof before any recovery IPC, even when foreground activation is acceptable.
- **References:** `codex-switcher/src/recovery/desktop_ipc.rs`, `codex-switcher/src/recovery/deferred_recovery_service.rs`, `README.md`, `CODEX.md`, `AGENTS.md`.
