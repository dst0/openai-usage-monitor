# 2026-09-28 — Empty restart targets hid the window banner

- **Status:** Partial
- **Task/context:** Showing a truthful selected-window restart panel when there are no running recovery targets.
- **Unexpected observation or failure:** A window could be captured for restart, yet the panel never appeared because the banner constructor returned early on an empty task list.
- **Evidence:** `RecoveryBanner::start_with_placement` returned a hidden banner for zero IDs; the distribution path with window preservation disabled also bypassed WindowServer capture when `targets` was empty. The helper can render a payload with zero task rows.
- **Approaches tried:**
  - **Attempt:** Add a fabricated task row to the recovery targets.
    - **Outcome:** Rejected.
    - **Why:** A selected window is not proof of a running or resumed task and must not enter owner-routed IPC.
  - **Attempt:** Give an empty task catalog its own one-window text while keeping the task list empty.
    - **Outcome:** Local Rust payload test passes; installed-app display remains unverified.
    - **Why:** The panel reports the captured window count without inventing task work.
- **Root cause:** Banner visibility was coupled to the number of recovery targets, although window restoration can occur with zero such targets.
- **Resolution:** A captured restart window can create a generic panel with zero task rows; the no-preservation path attempts the same read-only capture. The post-relaunch target auth and saved Desktop session are checked before panel rebinding or geometry restoration.
- **Verification:** Rust payload test confirms the one-window wording, no task rows or resume claim, and accurate no-preservation text. Focused installed-app UI validation is pending.
- **Prevention/follow-up:** Exercise both preservation modes in the installed Desktop with zero running recovery targets before claiming visible end-to-end behavior.
- **Reusable learning:** Keep window restart visibility separate from task recovery eligibility and never invent a task row to make a panel appear.
- **References:** `codex-switcher/src/recovery/recovery_banner.rs`, `codex-switcher/src/recovery_banner/recovery_banner_payload.rs`, `codex-switcher/src/distribution/system_app_lifecycle.rs`.
