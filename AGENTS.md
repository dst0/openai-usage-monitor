# Codex Switcher & Monitor — Agent Guidance

## Core Architecture
- `codex-switcher/`: Rust CLI (`cxi`, `codex-mon`) for quota monitoring, account switching, and automated thread resumption.
- `Sources/`: Swift AppKit Menu Bar application (`Codex Monitor.app`).
- `/Applications/ChatGPT.app`: the standard OpenAI Codex Desktop application. Its bundled `codex app-server` is the only Desktop thread writer; the Monitor does not install or launch a second one.
- `~/.codex/ipc/ipc.sock`: same-user Desktop IPC router used by recovery to address the window that owns a task.
- `~/.codex/app-server-daemon/`: Desktop-owned app-server coordination state; it is shared with the official app and is preserved by the Monitor uninstaller.
- Installation may place the Monitor bundle in `/Applications` or `~/Applications` depending on write access; a system install also leaves a `~/Applications/Codex Monitor.app` symlink.
- `~/.codex/auth.json`: Active credentials used by Codex CLI and ChatGPT.app.
- `~/.codex/accounts.json`: Multi-account credentials and quota cache (POSIX 0600 permissions).
- `~/.codex/usage-status.json`: Real-time cache consumed by the Swift status bar app.
- `~/.codex/thread-writer-locks/`: Active flock files held by running Codex app-server worker threads.
- `~/.codex/state_5.sqlite`: SQLite database tracking thread metadata (`updated_at`, `rollout_path`, `archived`, `thread_source`).
- `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`: Event logs for all thread turns and tool executions.

### Installation & Removal Contract

- `scripts/install.sh` installs the Monitor/Switcher components only; the
  official `/Applications/ChatGPT.app` and its bundled app-server are neither
  patched nor replaced. It may use `/Applications` or `~/Applications` for the
  Monitor bundle and retains remote-install skill sources in
  `~/.local/share/codex-monitor/`.
- `scripts/uninstall.sh --dry-run` previews cleanup. The confirmed uninstall
  removes the Monitor footprint, launch items, helper, notifier, skills, logs,
  and runtime state while preserving Desktop-owned `auth.json`, databases,
  sessions, IPC, writer locks, and source checkouts. `--purge-data` is required
  before removing the Monitor-owned `accounts.json` registry.
- “No traces” means no persistent Monitor-owned installation artifacts. Shell
  history, unified logs, LaunchServices/TCC records, APFS snapshots, and
  backups are outside the app's ownership and are not forensic-erased.

## Invariants
- POSIX `0600` permissions on all credential and token files (`auth.json`, `accounts.json`).
- Never print or log tokens/secrets to stdout/stderr.
- Always use atomic file operations (`fs2` flock) when writing `auth.json` or `accounts.json`.
- Keep binary memory overhead strictly under 5 MB RAM.

---

## Thread Detection & Resumption Engine

When switching accounts (via CLI `cxi switch` or Menu Bar app), eligible
quota-paused and restart-captured user threads are discovered and resumed under
the new account's credentials. Discovery-only recovery fails closed for an
ambiguous active turn instead of dispatching a duplicate request.

### 1. Two-Phase Detection Pipeline (`detect_in_progress_threads`)

The detector scans threads using two distinct layers:

1. **Phase 1: Active Process Locks (`~/.codex/thread-writer-locks/`)**
   - Examines all `*.lock` files in `thread-writer-locks/`.
   - Tests if the file is exclusively locked by a running worker process (`file.try_lock_exclusive().is_err()`).
   - Ignores sub-agent spawned threads (`thread_source == 'subagent'`).
   - Verifies the rollout tail state: only includes threads that are mid-turn (`ActiveInProgress`) or recently halted by quota (`InterruptedByQuota`). Cleanly completed or user-aborted turns are never resumed.

2. **Phase 2: Historical SQLite & Quota Exhaustion Scan (`state_5.sqlite`)**
   - When a turn hits a rate limit or runs out of credits, ChatGPT's app-server emits `task_complete` with an error and **releases its lock file**. Thus, Phase 1 alone will miss quota-exhausted threads.
   - Queries `state_5.sqlite` for the top `N` most recently updated unarchived user threads.
   - For each thread, checks if it failed due to quota exhaustion within the configured time window (`RECENT_QUOTA_WINDOW_SECS`).

### 2. Constants, Thresholds & Limits

| Constant / Parameter | Value | Rationale & Impact |
| :--- | :--- | :--- |
| `RECENT_QUOTA_WINDOW_SECS` | `14400` (4 hours) | Maximum lookback age for resuming quota-exhausted threads. Allows users to switch accounts hours after hitting limits without losing paused sessions. |
| `recent_threads` scan limit | `30` | Number of recent unarchived user threads queried from `state_5.sqlite` (`ORDER BY updated_at DESC LIMIT 30`). Prevents skipping active projects in multi-session environments. |
| `read_rollout_tail_lines` max bytes | `131072` (128 KB) | Reads only the tail of `rollout-*.jsonl` via `SeekFrom::Start(len - 128KB)`. Eliminates multi-second I/O stalls on large sessions (e.g. 500 MB+ files). |
| App shutdown cooldown | `600 ms` | Sleep after `pgrep` confirms process exit to allow macOS `LaunchServices` and `WindowServer` to clear registration before relaunching. |
| App launch verification | `3` attempts, up to `15 s` each + `2 s` settle | Uses `open -n -a /Applications/ChatGPT.app`, requires exactly one stable new main PID. |
| Desktop IPC startup | up to `120 s` | Waits for the standard Codex Desktop same-user IPC socket, validating owner, mode, peer UID, and socket identity. |
| Desktop owner discovery | up to `30 s` | Resolves the Desktop window that owns a task; a deep link is used only when no owner exists. |
| IPC recovery dispatch | `180 s` | Bounds the wait for Desktop to accept one owner-routed recovery request and start the expected turn. |
| Recovery execution | `600 s` | Allows a started task to produce substantive new agent work before failing closed. |
| Desktop stability | `10 s` | Requires the same singleton Desktop PID throughout, then verifies its visible window. |
| Banner minimum visibility | `5 s` | Keeps the recovery banner visible long enough to prevent flickering while dismissing promptly upon verification. |

### 3. Rollout State Classification (`ThreadRolloutState`)

Rollout files are evaluated backwards from the tail, filtering out post-turn metadata:
- **Filtered Metadata Events**: `item_completed`, `thread_settings_applied`, `token_count`, `token_usage_record`, `inter_agent_communication_metadata`. These do not alter turn lifecycle.
- **`InterruptedByQuota`**: The turn's final `task_complete` payload contains an `error` object matching:
  - `usage_limit_exceeded`
  - `workspace_owner_credits_depleted`
  - `out of credits`
  - `rate_limit_reached_type` (when not null)
  - Generic `credits`, `limit`, or `quota` error indicators.
- **`ActiveInProgress`**: The latest turn has initiated activity (`user_message`, `agent_message`, `reasoning`, `custom_tool_call`, `function_call`, `web_search`, `file_change`) without a closing `task_complete`.
- **`TurnAborted`**: The turn was interrupted by an app restart, process crash, or interruption (`turn_aborted`). A captured restart may recover it through Desktop-owned IPC; discovery-only recovery does not revive an ambiguous historical user Stop.

### 4. Desktop-Owned IPC & Queued Message Resumption

- **Standard Desktop contract**: The user installs and runs the official Codex Desktop app normally. Desktop starts its bundled app-server and exposes the same-user IPC router; no separate app-server installation, custom flags, or manual socket setup is part of this project.
- **Owner discovery**: `cxi` connects to `~/.codex/ipc/ipc.sock`, asks `thread-owner-discovery` for the task owner, and opens a `codex://threads/<tid>` deep link only when Desktop reports no owner for a cold task. Already-owned tasks are never cycled through the UI.
- **Interrupted turn dispatch**: For a task without a pending queue, `cxi` sends exactly one `thread-follower-start-turn` request to the owner with the protocol-valid text `continue` and `turnTrigger = app_update_resume`.
- **Queued follow-ups**: Existing queued payloads are preserved. `thread-follower-set-queued-follow-ups-state` is used to remove only the exact restart pause reason; user-paused queues are rejected.
- **Verification**: A dispatch/IPC acknowledgement is not success. Proof requires the exact returned turn ID, a post-checkpoint `task_started`, substantive agent work, a 10-second error-free soak, and Desktop stability/visibility verification.
- **Accessibility scope**: The native helper may show the recovery banner and verify a visible Desktop window for the exact PID. It is not used to click Play, Resume, Retry, or Steer controls to dispatch recovery.

---

## Auto-Switching Engine & Policies

The switcher daemon (`daemon.rs`), CLI switcher (`strategy.rs`), and Menu Bar app (`AppDelegate.swift`) enforce unified account rotation policies:

### 1. Trigger Conditions (`needs_switch`)
An account switch is triggered when:
- Active account 5-hour sprint quota reaches `0.0%` or encounters an HTTP `429` / rate limit error.
- **Preemptive Quota Restore Trigger**: Under `business_priority` mode, if the active account is a personal account (`!is_business`) and any configured business account has recovered available quota (`> 0%` and error-free), a switch is immediately triggered to return to the business account.

### 2. Candidate Selection & Sorting (`select_best_switch`)
1. **Filtering**:
   - Accounts must have quota `> 0%` and no active error.
   - Candidates must provide strictly greater quota than the active account (unless triggering a preemptive return).
   - In `business_only` mode, non-business accounts are strictly excluded.
2. **Priority Ordering**:
   - **Business Tier First**: Under `business_priority`, business accounts (`Team`, `Business`, `Enterprise`) always precede personal accounts (`Plus`, `Pro`, `Free`).
   - **Reset Credits (`credits`)**: Candidates with available rate-limit reset credits are sorted first (`credits` descending).
   - **Reset Duration**: Candidates whose quota resets soonest are preferred (`reset_after_seconds` ascending).
   - **Headroom**: Highest available percentage (`fiveHourPercentage` descending).

---

## Plan Multipliers & Capacity Mathematics

- **Auto-detected Tiers**: `Plus` (1.0x), `Pro` (2.0x), `Team` (2.0x), `Business` (2.0x).
- **Custom Overrides**: Set via `cxi set-multiplier <account> <val>` (e.g. 5 for Pro 5x / Business Premium, 20 for Pro 20x). Stored in `Account.plan_multiplier` in `accounts.json`. Clear with `cxi reset-multiplier <account>`.
- **Effective Multiplier**: Evaluated via `acc.effective_multiplier()`.
- **Tank Normalization**:
  - Normalized tank percentage: `(percentage / multiplier).clamp(0.0, 100.0)`.
  - Progress bars scale to `100.0 * plan_multiplier`.

---

## macOS Status Bar Visual & Rendering Invariants

- **Anti-Vibrancy Invariant (Composite NSImage)**: AppKit text rendering on inactive displays applies `NSTitlebarContainer` vibrancy that washes out custom text colors. To prevent this, the Menu Bar app renders a composite `NSImage` offscreen with explicit CGContext alpha blending and applies `.imagePosition = .imageOnly` on inactive screens.
- **3D Stratified Shield Badge**:
  - Total badge height: 16.5 pt, width: 6.5 pt (single-provider column, exactly half of AGY dual-model 13.0pt badge).
  - Top Tier (5h Sprint): Tallest section (7.5 pt of 16.5) with linear gradient based on sprint quota.
  - Middle Tier (Weekly Pool): Middle section (5.8 pt of 16.5). If weekly pool is exhausted or critical, overrides top tier green to gray to prevent misleading operational indicators.
  - Bottom Strip (Reset Credits): Compact strip (3.2 pt of 16.5). Green when credits > 0.
  - Outer Rim: Crisp, thin dark outer stroke (`lineWidth: 0.6`, `rimAlpha: 0.85/0.80`).
- **Contrast Shadows**: Specialized contrast shadows for brackets `[ ]` and omnidirectional soft red shadow for low-quota alerts to ensure legibility across all wallpaper luminosities.
