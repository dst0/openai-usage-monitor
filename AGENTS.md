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
| App server init sleep | `3000 ms` | Sleep before queuing resumption messages to allow the bundled `codex app-server` to start and accept CLI socket commands. |
| UI cycle switch delay | `400 ms` per thread | Delay after invoking `open codex://threads/<tid>` before triggering accessibility inspection. |
| UI Accessibility polling | `3` attempts @ `200 ms` | Polling interval for locating and clicking the native "Resume" / "Retry" button via AXUIElement. |
| Primary thread UI focus | `6` attempts @ `400 ms` | Extended polling to ensure the primary/current thread is frontmost and active when switching finishes. |

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
- **`CleanCompleted`**: The last turn completed with `task_complete` with no error, or a non-quota execution error. **Never auto-resumed.**
- **`TurnAborted`**: The turn was explicitly cancelled by the user (`turn_aborted`). **Never auto-resumed.**

### 4. UI Hydration Quirk & Solution

- **The Problem**: ChatGPT.app on macOS is an Electron/Chromium application. When a turn halts due to rate limits, the frontend displays an interrupted state banner and stops processing. If the credentials in `auth.json` are swapped in the background, background tabs do NOT automatically wake up; they remain idle until the user focuses or clicks on them.
- **The Solution (UI Cycling)**:
  1. `cxi` sends `continue` to each thread via `/Applications/ChatGPT.app/Contents/Resources/codex queue --thread <tid> --message continue`.
  2. `cxi` cycles through each thread URL (`open "codex://threads/<tid>"`) with a `400 ms` delay.
  3. For each tab opened, `cxi` triggers macOS Accessibility APIs (`AXUIElementCopyAttributeValue`) to press any unpause/resume buttons.
  4. Finally, `cxi` refocuses the user's primary/active thread.
