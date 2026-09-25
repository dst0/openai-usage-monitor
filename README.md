# 🚀 OpenAI Codex Multi-Account Monitor & Switcher

Ultra-lightweight, high-performance automatic quota monitoring and account rotation system for **OpenAI Codex** (supporting both console **Codex CLI** and desktop **Codex / ChatGPT.app**).

Engineered with **100% functional parity** and zero-overhead performance: core in **Rust** (~3 MB RAM footprint, instantaneous execution) paired with a native macOS Menu Bar status application in **Swift** (`Codex Monitor.app`).

> [!IMPORTANT]
> **🍎 Platform Notice: Currently works exclusively on macOS (Apple Silicon M1/M2/M3/M4 & Intel, macOS 13.0 Ventura or later).**
> Linux and Windows headless CLI support is planned for future updates.

---

## 🎯 Key Features

1. **5-Hour Rolling Sprint Monitoring (`wham/usage`)**:
   - Precise quota calculation (`100% - used_percent`) for the primary 5-hour rolling window (`limit_window_seconds: 18000`).
   - Exact countdown timer until rate-limit reset (e.g. `↻ 4h 45m`).
   - Tracking of the 7-day secondary limit window (`secondary_window: 604800s`) and rate-limit reset credits (`rate_limit_reset_credits`).

2. **Intelligent Auto-Switching Policies & Quota Recovery**:
   - **Reset-First & Highest Quota**: Automatically selects the account resetting soonest with maximum available headroom.
   - **Reset Credit Priority**: Accounts with available rate-limit reset credits (`credits`) are prioritized first.
   - **Business-Only Mode**: Restricts automated switching exclusively to corporate/team accounts (Team, Business, Enterprise).
   - **Business-Priority Mode**: Exhausts business quotas first, with automatic **preemptive return** to business accounts the moment their quota restores!
   - Atomically updates `~/.codex/auth.json` protected by `fs2` file locks (`flock`) and strict POSIX `0600` permissions.

3. **Plan Multiplier & Capacity Scaling**:
   - Auto-detects and scales capacity for OpenAI plan tiers (1.0x Plus, 2.0x Team/Business, up to 20x Pro).
   - Manual override per account via `cxi set-multiplier <account> <val>` (and `cxi reset-multiplier`).
   - Proportional quota visualization in CLI and Menu Bar (`PLAN (MULT)` and `5H SPRINT (EQ)`).

4. **Instant Switching for Codex CLI**:
   - Codex CLI reads `~/.codex/auth.json` on each invocation.
   - Transparent `cxi` / `codex-mon` shim or `codex` wrapper ensures agent swarms and terminal sessions never fail with `429 Rate Limit Exceeded`.

5. **Desktop Application Switching (`ChatGPT.app`)**:
   - The desktop app (`/Applications/ChatGPT.app`, bundle ID `com.openai.codex`) shares the `~/.codex/auth.json` credentials.
   - When an account is switched, the tool gracefully restarts the desktop app (`restart_app_on_switch: true`), immediately updating the interface and active sessions to the new account.
   - If Desktop has no eligible standard window before a restart, automatic switching continues through the exact main process and Desktop IPC without window geometry restore or a banner. Accessibility failures, malformed geometry, and process identity mismatches still stop the switch before credentials change.

6. **Automated Session & Thread Resumption Across Switches**:
   - Detects eligible mid-turn tasks captured for a restart and threads halted by rate limits or credit exhaustion within the last 4 hours (`RECENT_QUOTA_WINDOW_SECS = 14400s`); discovery-only recovery does not guess about an ambiguous active turn.
   - Scans up to 30 recent threads via `state_5.sqlite` with instantaneous 128 KB tail reads (`read_rollout_tail_lines`), eliminating I/O stalls even on 500 MB+ session files.
   - Resumes through the official Codex Desktop owner's IPC connection to the Desktop-bundled app-server; it never launches a second/headless app-server, uses `codex exec resume`, or clicks UI controls. For an interrupted turn it sends one protocol-valid text input, `continue`, through `thread-follower-start-turn`.
   - Shows a verified semi-transparent banner when an eligible window and recovery target are present, including when exact window restoration is disabled. Requires a new exact-ID `task_started`, real agent work, and a 10-second error-free observation window before reporting success.
   - Filters out internal subagent threads and never resumes cleanly completed or user-aborted tasks.

7. **Native macOS Menu Bar App (`Codex Monitor.app`)**:
   - Official Codex icon in the status bar with composite `NSImage` rendering to bypass AppKit vibrancy on inactive displays.
   - Dual-session live display: `APP 97% | CLI 97% (↻ 4h 45m)` with contrast shadows and red anti-washout glow.
   - Distinctive 3D shield badges `[ 🛡️ ] 🛡️ 🛡️` with a 3-tier visual gauge (Top = 5h sprint, Center = 7-day pool, Bottom = reset credits strip) and 0.6pt crisp dark outer rim.
   - Rich dropdown menu:
     - **Block 1**: 🖥️ Codex Desktop App (`ChatGPT.app`) — active account, status `[ACTIVE IN APP]`, sprint and weekly progress bars, credit balance.
     - **Block 2**: 💻 Codex CLI — active account, status `[ACTIVE IN CLI]`, remove button `✕`, progress bars, CLI model selection submenu (`gpt-5-5`, `gpt-5-4`, `o3`, `gpt-4.5`).
     - **Block 3**: 👥 Backup Accounts — 1-click instant switch buttons, individual progress bars, account removal.
     - **Auto-Switch Settings**: Live menu toggles for Auto-Switch, Business-Only, and Business-Priority modes.
   - Quick action `➕ Add Account via Terminal...`.
   - Configurable polling interval (1m, 5m, 15m, 30m) persisted in `UserDefaults`.
   - Desktop app restart button.
   - Built-in offline documentation with interactive menu bar simulator (`helps.html`).
   - Full native multilingual localization across Menu Bar status items, menus, and system dialogs (13 languages: EN, UK, RU, DE, FR, ES, IT, PT, PL, NL, JA, ZH-Hans, VI); the interactive guide provides 12 languages and intentionally omits Russian.
   - Launch at Login support (`Launch at Login`).

---

## 🛠 Installation & Setup

### ⚡ One-Line Quick Install (macOS Only)
Install and configure everything with a single terminal command:

```bash
curl -fsSL https://raw.githubusercontent.com/dst0/openai-usage-monitor/main/scripts/install.sh | bash
```

*Or using the explicit bash invocation:*
```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/dst0/openai-usage-monitor/main/scripts/install.sh)"
```

### 🛠 Manual Install (From Source)
```bash
git clone https://github.com/dst0/openai-usage-monitor.git
cd openai-usage-monitor
./scripts/install.sh
```

### 📋 Prerequisites (macOS)
The installation script checks and guides you through the prerequisites automatically:
1. **macOS 13.0+ (Ventura, Sonoma, Sequoia)** — Apple Silicon (M1/M2/M3/M4) or Intel.
2. **Apple Command Line Tools (`swiftc`)**:
   ```bash
   xcode-select --install
   ```
3. **Rust & Cargo** (for building the ultra-lightweight CLI core):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

### 🔌 Integration with the Official Codex Desktop App

The normal OpenAI Codex Desktop installation is the supported Desktop
integration. Install `/Applications/ChatGPT.app` using the official OpenAI
instructions and complete its normal first-run sign-in; no patching of
`ChatGPT.app`, separate App Server installation, custom App Server flags, or
manual IPC setup is required.

When Desktop is running, it starts the `codex app-server` bundled inside the
application. The monitor connects to Desktop's same-user socket at
`~/.codex/ipc/ipc.sock` and asks the Desktop window that owns a thread to start
or restore it. Desktop remains the only thread writer. The monitor never starts
another App Server and never runs a headless `codex exec resume` process.

The CLI quota monitor and account store can still be used without Desktop, but
Desktop restart and thread recovery require the standard Desktop application to
be installed and running. An optional read-only transport check is:

```bash
cxi recovery-preflight
```

### 🤖 What the Installer Does Automatically
1. **Compiles the Rust CLI (`codex-mon` / `cxi`)** with maximum release optimizations.
2. **Installs to `~/.local/bin`** and adds an exact shell `$PATH` block to `~/.zshrc` (and an existing `~/.bash_profile`).
3. **Configures the transparent CLI shim** at `~/.local/bin/codex`.
4. **Builds the native macOS Menu Bar application** (`Codex Monitor.app`) and installs it into `/Applications` when writable, otherwise `~/Applications` (a system install also creates a `~/Applications` symlink).
5. **Registers macOS Login Item** for seamless launch on Mac startup.
6. **Configures 24/7 background `launchd` daemon** (`~/Library/LaunchAgents/com.codex.switcher.plist`).
7. **Integrates AI Agent Skills** into `~/.codex/skills`, `~/.claude/skills`, and `~/.agents/skills`; a one-line remote install retains their source under `~/.local/share/codex-monitor/skills` because its temporary clone is removed.
8. **Builds the optional `Codex Notifier.app`** in `~/Applications`, the `codex-ui-resume` helper in `~/.local/bin`, and bundles the confirmation-gated uninstaller inside the Monitor app.
9. **Launches the Menu Bar app immediately.**

The installer does not install or modify the official Codex Desktop app and
does not create a second App Server. It only installs the Monitor/Switcher
components listed above and uses Desktop's existing local IPC when recovery is
requested.

---

## 🧹 Uninstall and Data Removal (macOS)

Deleting the `.app` bundle alone is not a complete uninstall: the Monitor also
has a Login Item, a `launchd` agent, CLI shims, a recovery helper, optional
notification helper, and app-owned files under `~/.codex/`. Use the repository
uninstaller so those integration points are stopped and removed together:

```bash
./scripts/uninstall.sh
```

You can also start it from the running Menu Bar app: open the dropdown,
choose `⛔ UNINSTALL CODEX MONITOR…`, and type `UNINSTALL` in uppercase. The
app feeds the bundled uninstaller the explicit `--yes` confirmation and then
terminates itself. The confirmation dialog includes an unchecked option to
also remove the Monitor account registry; leave it unchecked to preserve
`accounts.json`, or use the separate `--purge-data` command from Terminal.

Review the exact scope without changing anything with
`./scripts/uninstall.sh --dry-run`. The normal command asks you to type
`REMOVE`; use `--yes` only for an explicitly approved non-interactive run.

For a downloaded script, the equivalent is:

```bash
curl -fsSL https://raw.githubusercontent.com/dst0/openai-usage-monitor/main/scripts/uninstall.sh | bash -s --
```

The uninstaller is confirmation-gated. `--yes` is the explicit non-interactive
form, and `--purge-data --yes` additionally removes the Monitor-owned account
registry (including stored account copies in `~/.codex/accounts.json`) after
showing the exact scope. Runtime status/recovery files and logs are removed by
the normal uninstall; the account registry is kept unless the purge is explicit.

It removes only this project's installed components: `Codex Monitor.app`,
`Codex Notifier.app`, `cxi`/`codex-mon`/`codex`/`codex-ui-resume`, the
`com.codex.switcher` LaunchAgent and restart worker, the Monitor Login Item,
Monitor preferences, app-specific support/cache/saved-state directories, the
copied help file, Monitor logs/journals, the remote-install skill cache, and
skill links that point to this project. The script is idempotent and does not remove
anything merely because it happens to be named `codex` unless it is the exact
shim installed by this project.

By design, uninstall does **not** delete the official OpenAI Codex Desktop app,
its bundled app-server, shared `~/.codex/auth.json`, `state_5.sqlite`,
`queue_1.sqlite`, sessions, thread-writer locks, or a source checkout. Those
belong to Codex Desktop or the user and may contain credentials or conversation
history. Removing the Monitor also cannot restore an earlier `auth.json` value
after an account switch; sign in or select the desired account in official
Codex Desktop if needed.

This removes the persistent Monitor installation footprint. A normal macOS
user-space uninstall cannot promise forensic erasure of shell history,
unified/system logs, notification history, LaunchServices caches, or TCC
permission records; those are outside the app's owned data and are intentionally
not purged by default.

---

## 📖 CLI Commands (`cxi` / `codex-mon`)

### 1. Check Current Status and Quotas
```bash
cxi status
# Or force a live refresh from OpenAI API:
cxi status --refresh
```

Example output:
```text
📊 OpenAI Codex Accounts & Rate Limits (Strategy: reset-first)
---------------------------------------------------------------------------------------------------------
     ACCOUNT      EMAIL                      PLAN (MULT) 5H SPRINT (EQ)       RESET IN     7D LIMIT   CREDITS
---------------------------------------------------------------------------------------------------------
→ 🟢 main         dev@example.com            team   2x   ████████   97%       4h 45m       68%        3      
  🟢 backup       backup@example.com         plus   1x   ████████  100%       now          100%       0      
---------------------------------------------------------------------------------------------------------
💡 Switch account: `codex-mon switch <name|id>` | Set multiplier: `codex-mon set-multiplier <name> <val>`
```

### 2. Save Current Authenticated Session
```bash
cxi save-current main
```

### 3. Interactive Account Setup Wizard
```bash
cxi setup
```
Allows you to:
- Save your current session under any label (`main`, `work`, `team-1`).
- Initiate browser authentication via `codex login` to register additional accounts.
- Inspect quota status across all configured accounts.

### 4. Add Account via Browser Login
```bash
cxi add secondary
```

### 5. Manual Account Switching
```bash
cxi switch backup
# Without restarting the Desktop App:
cxi switch backup --no-restart
```

### 6. Rename / Label an Account
```bash
cxi rename backup "Work Backup"
# Clear custom label:
cxi rename backup --clear
```

### 7. Configure Auto-Switch Policies & Desktop App Sync
```bash
# Inspect current configuration values:
cxi config

# Toggle automated account switching on quota exhaustion:
cxi config --auto-switch-enabled true

# Restrict automated switching exclusively to business accounts (Team/Business/Enterprise):
cxi config --auto-switch-business-only true

# Prioritize business accounts first (fallback to personal, preemptive switch back on quota restore):
cxi config --auto-switch-business-priority true

# Enable/disable automatic ChatGPT.app restart on switch:
cxi config --restart-app-on-switch true

# Opt in to one reset credit when a recent user task is blocked at exactly 0%
# of the weekly pool. Zero means no time gate (the menu default once enabled):
cxi config --auto-reset-weekly-enabled true --auto-reset-weekly-min-hours 0

# Or require more than one day before the ordinary weekly reset:
cxi config --auto-reset-weekly-enabled true --auto-reset-weekly-min-hours 24
```

### 8. Set or Reset Account Plan Multipliers
```bash
# Set custom multiplier override (e.g. 20 for Pro 20x, 5 for Pro 5x / Business Premium):
cxi set-multiplier main 20

# Reset multiplier override back to auto-detected default:
cxi reset-multiplier main
```

### 9. Remove an Account
```bash
cxi remove backup
```

### 10. Run Commands with Automatic Quota Check & Switch
```bash
cxi wrap exec "fix bug in auth"
```

### 11. Resume Eligible Threads
```bash
# Verify/resume eligible quota-blocked or restart-captured tasks:
cxi resume

# Or resume a specific thread by ID or URL:
cxi resume 01a07d3c-3008-75c2-87a6-2c5c75f0e48b
cxi resume "codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b"

# Real restart plus verified recovery; no account change. Safe to invoke inside Codex:
cxi restart
```

Self-restart is handed to an independent one-shot launchd worker. A failed
underlying recovery is recorded as `WORKER_RESULT failed`, while the wrapper
exits cleanly so launchd cannot repeat the destructive restart. `RESTART_DISPATCHED`
means scheduled, not completed; the printed log records old/new app PIDs and
per-task `RECOVERY_VERIFIED` or `RECOVERY_FAILED` outcomes. Active logs remain
plain text for live tailing. The tiny atomic `desktop-recovery.json` stores only
pending task UUIDs and survives a killed worker.

The installer applies the same content-redaction boundary to pre-existing
Monitor-owned logs before it relaunches the app or daemon. It first verifies
that the exact Monitor app and daemon are stopped, including a fresh executable
and start-time check immediately before a Monitor PID receives `TERM`; an
in-flight one-shot restart worker is never killed and must finish before
migration begins. The new app bundle is copied to same-filesystem staging and strictly signature-
verified before writers stop, then activated through a rollback-capable rename.
The migration then rewrites only the three active streams, exact timestamped Brotli archives,
and exact `recovery-runs/restart-<operation>.log` files through fd-anchored,
no-follow opens. Each input line is capped at 1 MiB; malformed Brotli, invalid
UTF-8, oversized lines, symlinks, or concurrent mutation fail closed without
replacing the source. Unchanged sanitized files keep their inode, changed files
are atomically replaced at mode `0600`, and closed archives remain Brotli Q6.
Foreign files and official Codex Desktop session logs are never migrated.
Directory enumeration is capped at 4096 entries and 1 MiB of names; an I/O
error from directory iteration also fails closed rather than being accepted as
end-of-directory.
Normal Monitor append and rotation operations take a shared lifecycle lock;
historical migration takes the exclusive lock, preventing writes through a
retired inode during the upgrade window. Only installation creates that lock,
so an uninstall cannot be followed by a stale writer recreating a separate
unlocked inode; daemon startup validates an existing lock and never creates one.
After taking the flock, a waiting writer reopens the pathname and verifies the
same inode before writing. A committed app swap disarms rollback before best-effort backup
cleanup, preserving the installed new bundle if cleanup is interrupted.

### 12. Open Interactive Documentation
```bash
cxi helps
```

---

## 🧠 Smart Auto-Switch Engine & Policies

The switcher daemon and Menu Bar application implement an autonomous policy engine designed to prevent interruptions during long agent runs and interactive development:

### 🔄 Auto-Switch Modes

1. **Default Mode (`reset-first` / Highest Quota)**:
   - Evaluates all available accounts when active quota drops to 0% (or on HTTP 429 errors).
   - Selects candidates resetting soonest with available headroom.
   - **Reset Credit Weighting**: When multiple candidate accounts have available quota, accounts with available **Rate-Limit Reset Credits** (`credits > 0`) are selected first.

2. **Business-Only Mode (`auto_switch_business_only: true`)**:
   - Strictly confines automated switching to corporate/team accounts (`team`, `business`, `enterprise`).
   - Personal accounts (`plus`, `pro`, `free`) are never selected automatically, protecting private developer accounts from team batch workloads.

3. **Business-Priority Mode (`auto_switch_business_priority: true`)**:
   - Uses business accounts first, sorting by reset credits and reset time.
   - If all business accounts are depleted, gracefully falls back to personal accounts so work is not halted.
   - **Preemptive Quota-Restore Switch**: While temporarily executing on a personal fallback account, the background daemon constantly monitors the business accounts. The instant any business account's quota regenerates (> 0%), the system immediately and preemptively switches back to the business account, freeing up the personal account!

---

## 🔄 Automated Session & Thread Resumption Engine

When switching accounts or restarting the ChatGPT desktop app, ongoing turns and tasks interrupted by quota limits are automatically detected, recovered, and resumed under the newly activated account.

### ⚙️ Detection Pipeline & Invariants

Detection runs through a two-phase analysis pipeline before terminating or restarting the app:

1. **Phase 1: Running Worker Locks (`~/.codex/thread-writer-locks/`)**
   - Active worker threads hold an exclusive `flock` on their corresponding lock file in `thread-writer-locks/`.
   - The detector tests exclusivity (`file.try_lock_exclusive().is_err()`). If locked by a running process, the thread is inspected via rollout state.

2. **Phase 2: Historical Database & Quota Exhaustion (`state_5.sqlite`)**
   - When ChatGPT hits a rate limit or runs out of credits, the app-server issues `task_complete` with an error and **releases its file lock**.
   - The detector queries `state_5.sqlite` for recent unarchived user threads and checks if their turn halted due to quota or credit exhaustion.

### 📊 Constants, Thresholds & Timing Limits

| Parameter / Constant | Value | Purpose |
| :--- | :--- | :--- |
| `RECENT_QUOTA_WINDOW_SECS` | `14400` (4 hours) | Lookback window for resuming threads interrupted by quota/credit exhaustion. |
| `recent_threads` SQLite limit | `30` | Number of recent unarchived user threads queried from `state_5.sqlite` (`ORDER BY updated_at DESC LIMIT 30`). |
| `read_rollout_tail_lines` buffer | `131072` bytes (128 KB) | Tail seek window for inspecting `.jsonl` rollout events, avoiding reading entire multi-hundred MB logs into RAM. |
| App shutdown cooldown | `600 ms` | Grace period after `pgrep` exit for macOS `LaunchServices` cleanup. |
| App launch verification | `3` attempts, up to `15 s` each + `2 s` settle | Uses `open -n`, requires exactly one new exact-main PID, and rejects a PID that changes during settling. |
| Desktop IPC startup | up to `90 s` | Waits for the relaunched Desktop's same-user IPC socket, validating owner, mode, peer UID, and socket identity. |
| Owner discovery | up to `90 s` | Resolves the Desktop window that owns a task. A deep link is used only when no owner exists. |
| Pre-dispatch activity grace | `3 s` | Detects a task that the user or Desktop has already resumed before any command is sent. |
| Recovery verification | `90 s` to dispatch, `600 s` to produce work, then `10 s` soak | Requires the IPC-confirmed turn ID, substantive agent work, and no later abort/error; IPC acknowledgement is not success. |
| Desktop stabilization | `3 s` | Requires the same singleton main PID throughout; verifies the visible window only when one was captured before restart. |
| Banner minimum visibility | `5 s` | Keeps the semi-transparent recovery banner visible when an eligible window and recovery target were found. |

`launchd` can deny Accessibility reads to the background switcher even when an
interactive Terminal invocation of the same helper can inspect the window. The
default `preserve_window_bounds_on_restart=true` treats that denial as blocking:
no auth change or Desktop restart occurs. If automatic switching is more
important than restoring the exact prior window geometry, run
`cxi config --preserve-window-bounds false`. In this explicit mode the switcher
still validates the exact Desktop PID and birth identity before shutdown,
recovers eligible tasks through Desktop IPC, and verifies the relaunched
singleton PID. It uses the WindowServer's read-only geometry for banner
placement when a visible window and recovery target exist; this path does not
require Accessibility. It skips window position/size restore and the
Accessibility-based visible-window check. Explicit WindowServer visibility or
geometry failures and panel visibility failures are logged without blocking
credential rotation; identity and helper protocol failures block it. Restore the setting with
`cxi config --preserve-window-bounds true` only after verifying that the
background helper can read the Desktop window.

### 🚦 Rollout Lifecycle States (`ThreadRolloutState`)

- **`InterruptedByQuota`**: The turn's final `task_complete` contains an `error` payload matching `usage_limit_exceeded`, `workspace_owner_credits_depleted`, `out of credits`, or active `rate_limit_reached_type`. **Automatically resumed.**
- **`ActiveInProgress`**: The latest event is a mid-turn event (`user_message`, `reasoning`, `custom_tool_call`, etc.) with no closing `task_complete`. A captured restart may resume it from its post-shutdown checkpoint. Discovery-only recovery refuses this ambiguous state so it cannot duplicate or stop a task the user already resumed.
- **`CleanCompleted`**: The last turn completed cleanly with no error, or a non-quota execution error. **Never auto-resumed.**
- **`TurnAborted`**: Ambiguous user/app interruption. Auto-recovered only when captured in the pre-restart manifest; an explicit `cxi resume <id>` can also recover it. Historical user Stop actions are not automatically revived.
- **Filtered Metadata**: Events such as `thread_settings_applied`, `item_completed`, and `token_count` are filtered out during tail inspection so they never mask or falsify turn completion states.

### 💡 Desktop-Owned IPC Recovery

The monitor is a remote-control client of the Desktop runtime that already owns each task. It never starts `codex exec resume`, which would compete for the writer lock and can produce “This is open in another app.”

The recovery algorithm is:

1. Detect eligible, unarchived non-subagent tasks and atomically journal their IDs before shutdown. Stale manifest IDs and a caller-provided primary task are revalidated against SQLite and can never force an internal subagent into recovery.
2. Gracefully stop Desktop, wait for the exact main process to exit, then record a second rollout checkpoint. This excludes old work and shutdown-flush events from recovery proof.
3. Relaunch Desktop, validate its same-user IPC socket, and resolve the owner of every task. Only ownerless cold tasks are opened once for mounting; already-owned tasks are never cycled through the UI.
4. Preserve any queued payloads exactly. Only the exact restart-generated pause reason is removed; user-paused queues are rejected. Otherwise send one `app_update_resume` turn-start request containing the short text `continue`. An uncertain send is never retried.
5. Bind proof to the exact turn ID returned by Desktop IPC. Require a post-checkpoint `task_started`, substantive agent reasoning/message/tool/web-search work, and then 10 seconds without an abort or error. An acknowledgement, writer lock, navigation, or start alone is not success.
6. Restore the primary task once only if recovery had to mount a different cold task, then require the relaunched singleton PID to remain unchanged for another 3 seconds. Verify its visible window when one was captured before restart. Recovery and account switching share an operation lock and the same pipeline.

On the current ChatGPT.app build, macOS may accept a `codex://threads/<id>`
request for a cold task without mounting it in a Desktop window. The switcher
then reports `RECOVERY_INCOMPLETE` with `no-client-found`; it does not send a
turn to an unverified owner. Opening that task through ChatGPT's own task
navigation and then running `cxi resume <id>` can recover an interrupted turn.
Check each task's actual state first: a task that completed independently must
not receive another resume request.

### ♻️ Account-Bound Weekly Reset Credits

The weekly reset option is off by default and is intentionally independent of account rotation. When enabled, the daemon acts only when all of the following are true: the active account's fresh weekly availability is exactly `0%`, a reset credit is available, the selected strict remaining-time threshold is met, and a recent (up to four hours), unarchived, user-owned task ended with a quota error. It never spends a credit for an idle account, an active task, an aborted task, or a subagent.

The monitor writes a private, atomic `~/.codex/auto-reset-state.json` journal before requesting a reset. It contains an opaque idempotency key, account/window marker, and task ID, is mode `0600`, and is deliberately not a log. A timeout or unknown result is retried only with that same key; a successfully applied reset is never consumed again for the same weekly window.

The reset is account-scoped and does not participate in thread ownership:

1. After taking the same operation lock as account switching, the monitor reloads the active account and revalidates its exact weekly exhaustion, selected threshold, reset-credit count, and account routing ID.
2. It sends one authenticated request to the ChatGPT reset service used by Codex, with the active account header and the journaled idempotency key. No token, email, or response body is logged.
3. `reset` and `already_redeemed` are treated as idempotent success. `nothing_to_reset` and `no_credit` permit normal auto-switch fallback. A transport failure or unknown response retains the same key and suppresses switching until the result is settled.
4. Only after a confirmed success does the monitor use Desktop's existing owner-routed IPC recovery path to resume the blocked task(s).

The monitor never starts a second app-server and never asks another runtime to load the task. Desktop remains the only thread writer; the direct service call is limited to the account-level reset operation. Desktop does not need a custom reset IPC handler.

The Monitor owns the launchd daemon lifecycle. Explicit Quit writes a private durable cancellation marker and unloads the recurring daemon. A scheduled worker that has not crossed the shutdown boundary stops; a worker already between shutdown and relaunch is allowed to restore Codex to a safe running state, but cannot begin another restart cycle. Starting Monitor clears the stale cancellation marker.

---

## 📁 Configuration Structure

The Monitor stores its account registry, status cache, and recovery journals in
`~/.codex/`, alongside files owned by official Codex Desktop:

### Monitor-owned (runtime removed by uninstall; account registry needs `--purge-data`)

- `~/.codex/accounts.json` — Stored multi-account credentials and cached quotas (strict `0600` permissions); removed only with `--purge-data`.
- `~/.codex/usage-status.json` — Real-time quota snapshot consumed by the macOS Menu Bar app; removed by the normal uninstall.
- `~/.codex/monitor.lock`, `daemon.lock`, `codex.lock` — Monitor coordination locks; removed when not held.
- `~/.codex/auto-reset-state.json`, `desktop-recovery.json`, `desktop-recovery.lock`, `desktop-automation-cooldown` — Private recovery/reset state removed by uninstall.
- `~/.codex/recovery-runs/`, `account-switcher-daemon.log`, and `account-switcher-daemon.err` — private Monitor recovery records and daemon logs; new output is redacted at write time and exact pre-existing Monitor log files are redacted during installation before writers restart; removed by uninstall.
- `~/.codex/helps.html` — Copied offline interactive documentation guide removed by uninstall.
- `~/.local/share/codex-monitor/` — Retained skill source used by one-line remote installs; removed by uninstall.

### Shared with official Codex Desktop (never removed by the uninstaller)

- `~/.codex/auth.json` — Active credentials used by Codex CLI and `ChatGPT.app` (strict `0600` permissions).
- `~/.codex/ipc/`, `app-server-daemon/`, `state_5.sqlite`, `queue_1.sqlite`, `sessions/`, and `thread-writer-locks/` — Desktop IPC, app-server, thread, queue, session, and writer-lock state.
- `~/.codex/config.toml` — User configuration; the uninstaller removes only the exact three-line skill entries that point to this installation's `~/.codex/skills/{cxi,codex-mon}/SKILL.md` paths.

---

## 🌐 Multilingual Support (13 App Languages; 12 Guide Languages)

The native macOS Menu Bar application (`Codex Monitor.app`) supports **13
languages**. The offline interactive guide (`helps.html`) supports **12** of
them; Russian is intentionally app-UI-only. Both use automatic locale
detection and resilient fallbacks:

| Code | Language | Native Name | Menu Bar UI & Strings | Interactive Guide (`helps.html`) |
| :--- | :--- | :--- | :---: | :---: |
| `en` | English | English | ✅ | ✅ |
| `ja` | Japanese | 日本語 | ✅ | ✅ |
| `zh-Hans` | Simplified Chinese | 简体中文 | ✅ | ✅ |
| `vi` | Vietnamese | Tiếng Việt | ✅ | ✅ |
| `uk` | Ukrainian | Українська | ✅ | ✅ |
| `de` | German | Deutsch | ✅ | ✅ |
| `fr` | French | Français | ✅ | ✅ |
| `es` | Spanish | Español | ✅ | ✅ |
| `it` | Italian | Italiano | ✅ | ✅ |
| `pt` | Portuguese | Português | ✅ | ✅ |
| `pl` | Polish | Polski | ✅ | ✅ |
| `nl` | Dutch | Nederlands | ✅ | ✅ |
| `ru` | Russian | Русский | ✅ | — *(app UI only)* |

- **Automatic Locale Detection**: `Codex Monitor.app` inspects `Locale.preferredLanguages` with priority prefix matching (e.g. `ja-JP` → `ja`, `zh-Hans-CN` / `zh-CN` → `zh-Hans`, `vi-VN` → `vi`).
- **Interactive Guide Selection**: The guide automatically resolves the active language via URL query parameter (`helps.html?lang=ja`), hash anchor (`#ja`), localStorage preference, or browser navigator languages, with an instant-switch dropdown selector in the navigation header.
