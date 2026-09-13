# Codex Switcher & Monitor — Agent Guidance

## Core Architecture
- `codex-switcher/`: Rust CLI (`cxi`, `codex-mon`) for quota monitoring, account switching, and automated thread resumption.
- `Sources/`: Swift AppKit Menu Bar application (`Codex Monitor.app`).
- `~/.codex/auth.json`: Active credentials used by Codex CLI and ChatGPT.app.
- `~/.codex/accounts.json`: Multi-account credentials and quota cache (POSIX 0600 permissions).
- `~/.codex/usage-status.json`: Real-time cache consumed by the Swift status bar app.
- `~/.codex/thread-writer-locks/`: Active flock files held by running Codex app-server worker threads.
- `~/.codex/state_5.sqlite`: SQLite database tracking thread metadata (`updated_at`, `rollout_path`, `archived`, `thread_source`).
- `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`: Event logs for all thread turns and tool executions.

## Invariants
- POSIX `0600` permissions on all credential and token files (`auth.json`, `accounts.json`).
- Never print or log tokens/secrets to stdout/stderr.
- Always use atomic file operations (`fs2` flock) when writing `auth.json` or `accounts.json`.
- Keep binary memory overhead strictly under 5 MB RAM.

---

## Thread Detection & Resumption Engine

When switching accounts (via CLI `cxi switch` or Menu Bar app), any active or quota-paused threads must be automatically discovered, unpaused, and resumed under the new account's credentials.

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
| App launch verification | `3` attempts @ `300 ms` | Polling loop verifying ChatGPT.app has launched after `open -a /Applications/ChatGPT.app`. |
| App server init sleep | `4000 ms` | Sleep before queuing resumption messages to allow the bundled `codex app-server` to start and accept CLI socket commands. |
| UI cycle switch delay | `1000 ms` per thread | Delay after invoking `open codex://threads/<tid>` before triggering accessibility inspection. |
| UI Accessibility polling | `10` attempts @ `300 ms` | Polling interval for locating and clicking the native "Resume" / "Retry" / "Steer" button via AXUIElement. |
| Primary thread UI focus | `5` attempts @ `300 ms` | Extended polling to ensure the primary/current thread is frontmost and active when switching finishes. |

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
- **`TurnAborted`**: The turn was interrupted by an app restart, process crash, or interruption (`turn_aborted`). Handled via native UI inspection for the circular Play button (`isPlayButton`) to unpause paused queues without injecting 'continue' messages.

### 4. UI Hydration & Queued Message Resumption

- **The Problem**: ChatGPT.app on macOS is an Electron/Chromium application. When a turn halts due to rate limits or an app restart, the frontend displays an interrupted state banner (`Queue paused because you interrupted`) with a text button labeled `Resume`. **Crucially, clicking the text button in this banner does NOT work.** Furthermore, sending `codex queue --message continue` merely inserts a record into SQLite; on an idle thread, it does NOT automatically execute.
- **The Solution (Circular Play Button & Steer Dispatch)**:
  1. `cxi` cycles through each thread URL (`open "codex://threads/<tid>"`) with a `1000 ms` mount delay.
  2. For each tab opened, `cxi` inspects the Accessibility hierarchy with active-zone filtering (bottom 250pt, excluding historical scrollback tool retries).
  3. **Button Hierarchy & Target Selection**:
     - **1st Priority — Circular "Play" Button**: Located at the bottom-right of the composer (`[AXButton] title='' desc='Resume'`, circular ~29x28pt with a white right-facing triangle). Clicking this button directly unpauses the queue and initiates execution.
     - **2nd Priority — `Steer` Button**: Located on the queued message row (`[AXButton] title='' desc='Steer'`). If a queued `continue` is present, clicking Steer transitions it into an active turn (`thread/queue/start`).
     - **3rd Priority — Turn `Resume`/`Retry` Button**: Located at the bottom of an interrupted assistant response turn.
     - **Explicitly Skipped**: The non-functional text button inside the banner (`[AXButton] title='Resume' desc='' width>50`).
  4. Finally, `cxi` refocuses the user's primary/active thread.

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

