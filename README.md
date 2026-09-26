# 🚀 OpenAI Codex Multi-Account Monitor & Switcher

Ultra-lightweight, high-performance automatic quota monitoring and account rotation system for **OpenAI Codex** (supporting both console **Codex CLI** and desktop **Codex / ChatGPT.app**).

The switching and monitoring core is written in **Rust**, paired with a native macOS Menu Bar application in **Swift** (`Codex Monitor.app`). Cold-task mounting and exact multiwindow task restoration remain subject to the Desktop limitations described below.

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
   - Atomically updates `~/.codex/auth.json` protected by `fs2` file locks (`flock`) and strict POSIX `0600` permissions. Credential and account-registry writes stage through unpredictable `create_new` files with `O_NOFOLLOW`; first-run import and duplicate repair merge with a fresh registry under the same lock.

3. **Plan Multiplier & Capacity Scaling**:
   - Auto-detects and scales capacity for OpenAI plan tiers (1.0x Plus, 2.0x Team/Business, up to 20x Pro).
   - Manual override per account via `cxi set-multiplier <account> <val>` (and `cxi reset-multiplier`).
   - Proportional quota visualization in CLI and Menu Bar (`PLAN (MULT)` and `5H SPRINT (EQ)`).

4. **Instant Switching for Codex CLI**:
   - Codex CLI reads `~/.codex/auth.json` on each invocation.
   - Transparent `cxi` / `codex-mon` shim or `codex` wrapper ensures agent swarms and terminal sessions never fail with `429 Rate Limit Exceeded`.

5. **Desktop Application Switching (`ChatGPT.app`)**:
   - The desktop app (`/Applications/ChatGPT.app`, bundle ID `com.openai.codex`) shares the `~/.codex/auth.json` credentials.
   - Desktop and Codex CLI must target the same account because they read the same credential file, including when Desktop is closed and may launch later. A requested split is rejected before a recovery journal or credential write; a CLI-only credential change is rejected while Desktop runs.
   - Background quota polling reads access tokens without refreshing OAuth credentials or writing `auth.json`. Desktop may rotate its own refresh token; the switcher preserves that latest token in the account registry after the exact Desktop writer exits and before replacing the shared credential file. An expired inactive account needs an explicit sign-in.
   - Direct `cxi switch` keeps the requested account's stable ID across registry synchronization and rechecks the target's credentials after Desktop shutdown. It writes a private `direct-switch-journal.json` before replacing auth, with IDs and SHA-256 fingerprints but no tokens. The next direct switch or account distribution, under the recovery operation lock, clears an exact prior state or finishes a verified target auth/registry commit; changed state or a live credential writer blocks it. A post-stop error before auth replacement relaunches the previous Desktop only after exact prior auth/registry readback. The final registry commit updates only the active ID against the latest locked registry, preserving concurrent settings and account changes; a changed active identity, removed or reauthenticated target, or newly invalid target blocks commit. Existing auth is compare-written, then checked as a full document after registry commit. Switching drops a prior API key and prior-account token extensions while retaining top-level Desktop extensions. If no auth file existed, creation cannot overwrite a newly appeared file and rollback restores absence.
   - Re-login requires a usable decoded email claim from the official CLI login matching the selected account and a non-default workspace ID; a workspace ID alone cannot identify a user. Conflicting ID/access token email claims fail closed. JWT signatures are not verified locally. Active-auth replacement checks file identity, contents, and running credential writers immediately before writing. ChatGPT does not honor the Monitor's advisory lock, so concurrent Desktop launch remains an unsupported race.
   - With `restart_app_on_switch: true`, the tool gracefully restarts the desktop app under the selected account. Eligible tasks are resumed only after Desktop mounts their owners and the recovery checks pass; a switch can therefore finish with partial recovery.
   - Before any restart, it checks the exact ChatGPT process and WindowServer window inventory. A process with multiple user windows, or an ambiguous inventory, blocks the restart before credentials change until exact window-to-task restoration is available. This applies even when window-bound preservation is disabled.
   - Window inventory cross-checks named WindowServer windows against Accessibility standard-window frames. An unnamed offscreen window is excluded only if its frame is distinct from every standard-window frame; an unexpected or malformed offscreen title blocks shutdown. This preserves the observed single-window Desktop with a detached renderer, but the completeness of Accessibility's window roster is still unproven.
   - The Monitor binds the displayed APP account to the exact relaunched process before waiting for task recovery.
   - Keep automatic switching disabled until unattended cold-task mounting and exact selected-task restoration across windows are proven on the installed Desktop.
   - If Desktop has no eligible standard window before a restart, automatic switching can continue without window geometry restore. If there are recovery targets, owner-routed IPC waits for a visible banner after owner mounting; a missing window then defers the target with its original checkpoint. Accessibility failures, malformed geometry, and process identity mismatches still stop a preservation-enabled switch before credentials change.

6. **Automated Session & Thread Resumption Across Switches**:
   - Detects eligible mid-turn tasks captured for a restart and threads whose latest quota-error `task_complete` rollout event occurred within the last 4 hours (`RECENT_QUOTA_WINDOW_SECS = 14400s`); discovery-only recovery does not guess about an ambiguous active turn.
   - Scans up to 30 recent threads ordered by `state_5.sqlite` `updated_at`, then checks quota age from the rollout event timestamp. Bounded 128 KB tail reads (`read_rollout_tail_lines`) avoid reading entire large session files during discovery.
   - Resumes through the official Codex Desktop owner's IPC connection to the Desktop-bundled app-server; it never launches a second/headless app-server, uses `codex exec resume`, or clicks UI controls. For an interrupted turn it sends one protocol-valid text input, `continue`, through `thread-follower-start-turn`.
   - Shows a verified semi-transparent banner when an eligible window and recovery target are present, including when exact window restoration is disabled. Requires a new exact-ID `task_started`, real agent work, and a 10-second error-free observation window before reporting success.
   - Recovery in an already running ChatGPT uses read-only WindowServer geometry for its banner. If the first capture finds no window, it tries again after Desktop confirms the task owner. A visible, live panel and unchanged queue/rollout are required before IPC; helper and process identity are checked again after SQLite waits, followed by a final queue/rollout check. Failed status replay into a late banner also blocks dispatch. Failures retain the original checkpoint for a later attempt. Window access/geometry failure, panel timeout, identity changes, missing helper, payload/lease failure, and malformed helper responses fail closed before dispatch.
   - Filters out internal subagent threads and never resumes cleanly completed or user-aborted tasks.

7. **Native macOS Menu Bar App (`Codex Monitor.app`)**:
   - Official Codex icon in the status bar with composite `NSImage` rendering to bypass AppKit vibrancy on inactive displays.
   - Dual-session live display: `APP 97% | CLI 97% (↻ 4h 45m)` with contrast shadows and red anti-washout glow.
   - APP and CLI percentages resolve independently. A running Desktop with no marker for its exact PID and birth identity displays `APP —` until a verified binding is written; it never borrows the CLI account or an old Desktop's quota. The Rust quota cache binds CLI identity to the matching active `auth.json` file identity, and Swift rechecks that identity before displaying a CLI percentage. An unknown or replaced CLI auth file displays `CLI —` instead of borrowing the first cached account or top-level quota. Distribution rechecks the CLI registry and authentication under its operation lock. APP and CLI switches use the same target account; a failed shared-auth or registry commit reports failure. Marker and Desktop lifecycle events refresh the menu without waiting for the next quota poll.
   - After an external ChatGPT relaunch, the daemon can renew a previous APP marker for the new exact process only when the same account's private shared auth predates that process, its complete tokens match the unique active registry entry, and file and process identities remain stable around the marker write. The inferred marker records the exact auth file identity; Swift hides APP when it changes. Missing evidence leaves APP as `—`. A same-process in-app account change without a matching auth-file change remains unverified.
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
   The selected Swift compiler and macOS SDK must come from a matching toolchain.
   Run `./scripts/test_swift.sh` before installing; if Swift reports an SDK/compiler
   version mismatch, repair or select matching Command Line Tools and rerun the
   test. Do not publish or reinstall a partially built app bundle.
   If you set `CLANG_MODULE_CACHE_PATH`, point it at a new, empty directory for
   each installation; a new directory under the canonical `/private/tmp/...`
   path is the verified workaround. Whether reusing a cache through the `/tmp`
   alias, or reusing a cache at all, caused the duplicate-module failure seen
   once is still unverified.
3. **Rust & Cargo** (for building the ultra-lightweight CLI core):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   The build uses the exact toolchain pinned in `codex-switcher/rust-toolchain.toml`;
   the installer asks rustup to install it before compiling.

### 🦀 Rust toolchain policy

`codex-switcher/rust-toolchain.toml` pins one exact Rust release (with Clippy
and rustfmt) so a new upstream `stable` cannot change build or lint results
without a reviewed commit. rustup applies the pin whenever cargo runs from inside
`codex-switcher/`, which is how the installer, the shell tests, and CI invoke it.
It does not apply to `cargo --manifest-path` run from another directory, or to a
Rust installed without rustup (the installer warns in that case).

CI installs the pinned toolchain with `rustup toolchain install`, resolves the
gitignored `Cargo.lock` with `cargo generate-lockfile`, then runs
`cargo clippy --workspace --all-targets --locked -- -D warnings` (every target,
tests included) before `cargo test`. `--locked` refuses to create a missing
lockfile, so run `cargo generate-lockfile` first in a fresh local checkout.

`codex-switcher/tests/ci_workflow_policy.rs` enforces the policy: it rejects
floating channels and per-command overrides in workflows, requires that exact
Clippy step in a required Rust job, rejects inherited variables, default
shells, and `GITHUB_ENV`/`GITHUB_PATH` references that could weaken it, and on
GitHub Actions asserts that the tests themselves ran under the pinned release.

To bump the toolchain:

1. Change `channel` in `codex-switcher/rust-toolchain.toml` to the new exact `X.Y.Z` release.
2. In `codex-switcher/`, run `rustup toolchain install`, then `cargo test` and
   `cargo clippy --workspace --all-targets --locked -- -D warnings` on the new
   toolchain; fix every lint the new release adds in the same PR, because the
   CI Clippy gate fails on any finding.
3. Open a dedicated `build/` PR and confirm every required check passes on its head SHA.

GitHub Actions in `.github/workflows/` are likewise pinned to full commit SHAs
with a `# vX.Y.Z` comment. To update one, resolve the release tag to its commit
(`gh api repos/<owner>/<action>/commits/<tag> --jq .sha`), confirm the tag points
at that commit, and update both the SHA and the comment.

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

ChatGPT Desktop and `cxi` share one `auth.json`. An APP/CLI split is unsafe
even while Desktop is closed because its next launch reads the CLI credential.
Both targets must select the same account; changing the CLI credential alone
is refused while Desktop runs. Unattended quota checks never perform an OAuth
refresh or rewrite the active credential file. Before a restart changes it,
the switcher verifies the exact Desktop process and its bundled app-server
writer have stopped, then saves any token Desktop refreshed during shutdown.
For accounts sharing an email, provider ID and email alone do not establish
credential ownership. Recovery, distribution, and direct switching reject a
nonblank access, refresh, or ID token already saved for a different account;
unique same-account token rotation remains eligible. An offline switch saves
the previous account's verified token rotation before replacing shared auth.
Before shutdown, the planned Desktop and CLI account identities must both
match the uniquely identified live authentication; a stale marker or registry
cannot authorize stopping Desktop. If saving a shutdown-time token rotation
fails, the previous Desktop is relaunched only after that live account is
uniquely verified, and the failed handoff remains journaled. An unreadable
process inventory or uncertain account identity stops the switch.
An emergency relaunch binds the previous account to the exact new Desktop
process and saves any same-account launch-time token rotation before clearing
its journal; it sends no recovery request for a failed checkpoint.
Journal inspection and candidate planning hold the same recovery operation
lock as the commit, so an older in-flight journal cannot be cleared by another
switch. Offline account changes recheck for a Desktop writer immediately before
replacing shared authentication; if the Desktop marker cannot be saved, the
switcher restores the previous auth and registry or retains the journal when
rollback is unverified. A stop error after signalling Desktop retains the
recovery journal and checkpoint. For a checkpoint preparation error or a stop
rejected before signalling, distribution clears its journal only after
restoring the exact previous recovery checkpoint; failed restoration retains
the journal for inspection.
The switcher also checks process state and auth readback after replacement and
again after offline registry/marker writes; the relaunched Desktop's registry
commit requires a final auth readback.
Immediately before any owner-routed recovery IPC, the switcher verifies that
live `auth.json`, the exact relaunched Desktop PID and birth identity, and its
saved session marker still identify the planned account. Any disagreement
blocks recovery dispatch.
Shutdown token handoff and post-relaunch registry commit merge into a fresh
`accounts.json` snapshot under the Monitor lock, preserving concurrent account
settings. Configuration and registration update only their fields in a fresh
registry transaction; quota HTTP runs outside that lock. Never load a registry
and later save its whole stale snapshot. Auth readback compares the full
document, including extension fields.
Active-auth reads require a private regular file opened without following
symlinks and recheck the named inode after reading.
Desktop account distribution replaces the complete target token object and
clears any API key belonging to the previous account. Forward replacement and
rollback compare the exact shared-auth document and file identity immediately
before writing; a concurrent credential change blocks the write and leaves the
journal for inspection when the outcome is uncertain.
An abandoned distribution journal in `stopping_desktop`, `auth_commit_app`,
`auth_commit_cli`, `relaunching_desktop`, or an unknown phase blocks the next
distribution, even when its worker PID is gone. Reconcile the live auth
identity, saved active account, recovery checkpoint, and Desktop session before
removing that journal; PID age alone is not proof of rollback.
The distribution journal is a bounded private state file: reads require a
same-user regular `0600` file of at most 16 KiB, opened without following
symlinks and checked against the named inode. Writes use random exclusive
same-filesystem `0600` staging, then atomic replacement and directory sync;
an unsafe existing path blocks distribution.
The Desktop session marker uses the same private bounded I/O: it is read as a
same-user regular `0600` file of at most 16 KiB with no symlink following and
a stable inode. A malformed or unsafe marker blocks account distribution;
writes use random exclusive staging and directory sync.
If marker replacement succeeds but the following directory sync fails, the
switch restores the previous marker alongside auth and registry before clearing
the distribution journal; an unverified restoration retains that journal.
Offline distribution updates only `active_account_id` in a
fresh locked registry, including on rollback, preserving concurrent settings
and account edits. Rollback also verifies that the previous account's saved
credentials still match the auth being restored; a changed binding retains the
journal and blocks a success claim.
Interactive account setup stops on an unreadable registry or active credential,
and uses a fresh private login directory instead of a predictable PID path.
`usage-status.json` is a derived cache: status writes take the registry lock and
copy the latest switch settings before replacement. The Menu Bar app treats a
missing or malformed auto-switch flag as disabled; if the cache is absent,
the daemon creates the next complete quota snapshot.
A manually consumed reset credit updates only that account's credit cache in a
fresh registry transaction. Changed credentials, account route, or credit count
make the post-consumption cache uncertain; the command reports that state and
does not send another reset request.
Before `cxi reset-account <account>` sends its request, it saves a private,
mode-`0600` `manual-reset-state.json` attempt with the account reference,
previous credit count, start time, and opaque idempotency key. A crash, unknown
service result, or confirmed consumption with a failed cache commit leaves the
attempt unresolved. Later manual reset commands stop before sending another
request, including for another account. A confirmed non-consumption or an
applied reset with a successful cache commit resolves the attempt and permits
a later explicit reset.
Manual and automatic reset requests share the recovery operation lock. A
pending or unknown automatic attempt blocks a new manual request for the same
account route even if its cached weekly timestamp changes. Any unresolved
manual attempt blocks automatic reset across local account IDs before it
creates another journal. Malformed, unreadable, or incomplete reset state also
blocks spending. An HTTP error without a parsed, authoritative result remains
uncertain, including 4xx responses. A restored weekly pool does not erase a
pending or unknown automatic attempt or release its account-switch suppression.
The automatic journal stores one attempt, so an unresolved attempt also blocks
automatic reset for another account until the original outcome is reconciled.
Keep automatic switching disabled until unattended cold-task mounting and
multiwindow task restoration are verified on the installed Desktop build.

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

# Fresh and legacy registries default to automatic switching disabled until
# cold-task recovery and window restoration are verified on this Desktop build.

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

Runtime and historical Monitor log redaction treats incomplete quoted
credentials and multiword unquoted sensitive values as opaque. It also scans wrapped
tokens and every email or UUID in list-valued diagnostics, so malformed helper
output cannot expose a later sensitive value in the same line.

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
   - The detector queries `state_5.sqlite` for recent unarchived user threads and checks whether the latest quota-error `task_complete` rollout event is within the eligibility window. SQLite `updated_at` orders candidates; it does not establish quota-error age.

### 📊 Constants, Thresholds & Timing Limits

| Parameter / Constant | Value | Purpose |
| :--- | :--- | :--- |
| `RECENT_QUOTA_WINDOW_SECS` | `14400` (4 hours) | Lookback window for resuming threads interrupted by quota/credit exhaustion. |
| `recent_threads` SQLite limit | `30` | Number of recent unarchived user threads queried from `state_5.sqlite` (`ORDER BY updated_at DESC LIMIT 30`). |
| `read_rollout_tail_lines` buffer | `131072` bytes (128 KB) | Tail seek window for inspecting `.jsonl` rollout events, avoiding reading entire multi-hundred MB logs into RAM. |
| App shutdown cooldown | `600 ms` | Grace period after `pgrep` exit for macOS `LaunchServices` cleanup. |
| App launch verification | `3` attempts, up to `15 s` each + `2 s` settle | Uses `open -n`, requires exactly one new exact-main PID, and rejects a PID that changes during settling. |
| Desktop IPC startup | up to `90 s` | Waits for the relaunched Desktop's same-user IPC socket, validating owner, mode, peer UID, and socket identity. |
| Owner discovery | up to `90 s` | Resolves the Desktop window that owns a task. An ordinary URL is sent only when no owner exists; one pinned native LaunchServices attempt follows after at least 10 s if still ownerless. |
| Pre-dispatch activity grace | `3 s` | Detects a task that the user or Desktop has already resumed before any command is sent. |
| Recovery verification | `90 s` to dispatch, `600 s` to produce work, then `10 s` soak | Requires the IPC-confirmed turn ID, substantive agent work, and no later abort/error; IPC acknowledgement is not success. |
| Desktop stabilization | `3 s` | Requires the same singleton main PID throughout; verifies the visible window only when one was captured before restart. |
| Banner minimum visibility | `5 s` | Keeps the semi-transparent recovery banner visible when an eligible window and recovery target were found. |

The tail reader checks the byte before its seek point. It discards a partial
first record before strict UTF-8 decoding, retains a full record at an exact
newline boundary, and reports unknown state for malformed complete records.

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
- **`InterruptedByError`**: The final `task_complete` carries a non-quota error (such as a 401 auth outage or a policy block) and no non-empty `last_agent_message`. Only explicit `cxi resume <id>` may continue it, including with queued work; unattended recovery refuses it and drops its retry intent. An error after a final agent message, including a non-null non-string final field, is treated as completed.
- **`CleanCompleted`**: The last turn completed cleanly, or a non-quota error followed a final agent message. **Never start another turn automatically.** A restart-captured queued follow-up may still be woken through its verified Desktop owner.
- **`TurnAborted`**: Ambiguous user/app interruption. Auto-recovered only when captured in the pre-restart manifest; an explicit `cxi resume <id>` can also recover it. Historical user Stop actions are not automatically revived.
- **Filtered Metadata**: Events such as `thread_settings_applied`, `item_completed`, and `token_count` are filtered out during tail inspection so they never mask or falsify turn completion states.
Tail reads stop at a captured file length and require a final newline. They
check the byte before the seek point, discard only a partial first record
before strict UTF-8 decoding, and retain a full record at a newline boundary.
A malformed newer record makes the state unknown and blocks unattended dispatch.

### 💡 Desktop-Owned IPC Recovery

The monitor is a remote-control client of the Desktop runtime that already owns each task. It never starts `codex exec resume`, which would compete for the writer lock and can produce “This is open in another app.”

When a target has queued follow-ups, recovery applies the same turn-state and
mode checks before and after owner discovery. It removes only Desktop's exact
restart pause reason. It sends one owner-routed queue-state update even when
the queue was already unpaused; mounting the task alone does not wake the
owner's coordinator. The recovery checkpoint is consumed immediately before
that IPC request, so an unknown send outcome is not retried automatically.

**Condition of use:** Configure only ChatGPT accounts owned by the same person using this device. Continuing that person's local tasks across their own accounts is an intended feature; do not register another person's account. Monitor still requires a verified Desktop owner under the selected account before it sends a recovery request.

The recovery algorithm is:

1. Detect eligible, unarchived non-subagent tasks and atomically journal their IDs before shutdown. Stale manifest IDs and a caller-provided primary task are revalidated against SQLite and can never force an internal subagent into recovery. Duplicate task IDs in the recovery journal fail validation. When recovery targets exist, handshake Desktop IPC before stopping ChatGPT; failure restores the prior checkpoint and leaves Desktop running.
2. Gracefully stop Desktop, wait for the exact main process to exit, then record a second rollout checkpoint. This excludes old work and shutdown-flush events from recovery proof. A deferred entry for the same task is replaced only after a newer turn has substantive, error-free work and no queued follow-up; unrelated deferred entries keep their account binding. The evidence scan reads a fixed rollout snapshot with bounded memory, including long turns. If the second checkpoint fails, previous shared authentication is staged for relaunch and its new process is bound to the same account; recovery requests are not dispatched. `cxi restart` also relaunches the previous Desktop state before reporting the error.
3. Relaunch Desktop, validate its same-user IPC socket, and resolve the owner of every task. While Desktop is already running, each task is routed to its own owner, including separate windows under one account. A restart with multiple windows is currently blocked before shutdown because their exact selected tasks cannot be reconstructed. Only ownerless cold tasks open a task URL; later `open -g -a /Applications/ChatGPT.app` retries continue, with one pinned native LaunchServices attempt after at least 10 seconds of initial ownerless waiting. Foreground activation is allowed. Already-owned tasks are never cycled through the UI. URL acceptance does not prove mounting: owner discovery remains mandatory before dispatch. Every recovery mode pins the uniquely verified active account and exact Desktop PID and birth identity at operation start, then rechecks both after owner discovery before and after durable checkpoint consumption. A pre-send mismatch restores the original checkpoint with readback and sends no IPC. Finalization preserves an explicitly claimed deferred target's original owner account and offset; a previously unbound target retains its offset but binds to the account that started recovery, blocking an unattended retry under the changed account. A hidden banner from a captured restart binds the relaunched process only through the exact live Desktop session marker.
4. Preserve any queued payloads exactly. Only the exact restart-generated pause reason is removed; user-paused queues are rejected. Otherwise send one `app_update_resume` turn-start request containing the short text `continue`. An uncertain send is never retried.
5. Bind proof to the exact turn ID returned by Desktop IPC. Require a post-checkpoint `task_started`, substantive agent reasoning/message/tool/web-search work, and then 10 seconds without an abort or error. An acknowledgement, writer lock, navigation, or start alone is not success.
6. Restore the primary task once only if recovery had to mount a different cold task, then require the relaunched singleton PID to remain unchanged for another 3 seconds. Verify its visible window when one was captured before restart. Recovery and account switching share an operation lock and the same pipeline.

On the current ChatGPT.app build, macOS may accept a `codex://threads/<id>`
request for a cold task without mounting it in a Desktop window. Historical
logs show successful URL-to-owner-to-IPC recovery. Live checks found some
accepted links that were ownerless at an immediate check, while a later owner
can appear asynchronously. Both `open -g -a` and native LaunchServices delivery
can bring ChatGPT to the foreground, which the owner permits. The installed
ChatGPT 26.924.20706 deep-link handler ensures the primary window is visible
before ordinary task navigation; no background cold-task mount IPC method was
evident. No recovery turn was dispatched in those checks. The switcher
reports incomplete recovery with `no-client-found` and keeps only these
pre-dispatch targets in its 0600 journal. The daemon probes periodically with
a 15-second minimum interval while no recovery is running and reissues an
ownerless task URL at most once per minute, alternating ordinary and pinned
native delivery after actual attempts. After a task gains
a Desktop owner, it retries recovery with the original checkpoint and the same
turn-progress verification. Deferred dispatch uses the Desktop session recorded
by a successful, exact-process relaunch; it checks the saved PID, birth identity,
and expected CLI account independently of the CLI account selected for Desktop.
Legacy sessions without that binding fail closed. The first banner closes after
bounded owner waits, and a fresh visible banner is required when a deferred
owner-routed recovery actually begins. It rechecks the
queue and rollout after owner discovery, durably clears retry intent before
any IPC request, and never retries a request whose outcome is unknown. A URL
launch or Desktop IPC startup failure before dispatch retains the checkpoint. A
manually resumed task retires its older ownerless retry only after a new turn
starts, produces agent work without an error, and has no queued follow-up; a
queued follow-up keeps its recovery intent. It drops completed tasks without queued follow-ups, archived, stale, or ambiguous
uncaptured targets. The pending
entry expires under the four-hour eligibility window measured from the quota-error
rollout event. External deep-link acceptance alone remains insufficient;
unattended end-to-end recovery of every cold quota task and exact selected-task
restoration across multiple windows are not verified on the current Desktop build.
The pinned native retry uses Apple's deprecated `LSOpenFromURLSpec` API as a
bounded fallback; a future macOS release may require a replacement.
Keep automatic switching disabled. If the daemon is
not running, inspect the task and use `cxi resume <id>` if it is still
interrupted.

Deferred probes use a bounded append cursor as a hint. Replacing or pruning a
saved ownerless retry as recovered work requires a fresh scan of one unchanged, newline-terminated
rollout snapshot; a partial, malformed, or oversized record in that interval cannot prove that a
new turn was error-free. One ownerless task is selected per prune pass with
rotation scoped to `CODEX_HOME`, without
tail-reading the other ownerless tasks. Foreground recovery scans at most 16 MiB
of rollout payload across its targets per pass, plus small boundary samples,
and waits for a complete post-checkpoint snapshot before sending IPC. If
previously scanned bytes could have changed during owner mounting, it retains
the checkpoint for a fresh attempt. The Desktop account and task owner are
verified afresh before every dispatch. Ordinary thread detection leaves
ownerless retries to the deferred worker.
For the one selected ownerless target, a stable terminal non-quota error drops
unattended retry intent because only explicit resume can continue that turn.
Malformed or changed tail state keeps the original intent.

### ♻️ Account-Bound Weekly Reset Credits

The weekly reset option is off by default and is intentionally independent of account rotation. When enabled, the daemon acts only when all of the following are true: the active account's fresh weekly availability is exactly `0%`, a reset credit is available, the selected strict remaining-time threshold is met, and a recent (up to four hours), unarchived, user-owned task ended with a quota error. It never spends a credit for an idle account, an active task, an aborted task, or a subagent.

The monitor writes a private, atomic `~/.codex/auto-reset-state.json` journal before requesting a reset, while holding the recovery operation lock. It contains an opaque idempotency key, account/window marker, and task ID, is mode `0600`, and is deliberately not a log. A timeout or unknown result is retried only with that same key when the original episode remains eligible; a successfully applied reset is never consumed again for the same weekly window. A corrected weekly timestamp or a different active account does not authorize replacing the sole unresolved journal. A restored quota snapshot cannot silently discard an unresolved attempt.

The reset is account-scoped and does not participate in thread ownership:

1. After taking the same operation lock as account switching, the monitor reloads the active account and revalidates its exact weekly exhaustion and window marker, selected threshold, reset-credit count, account routing ID, live CLI authentication, that a request can be built for the account route and token, and running Desktop. Only then does it write the `pending` journal entry, and the request follows immediately, so a change noticed during preparation never leaves an unsent attempt marked unresolved. If a check fails, or the request cannot be built, while retrying an attempt that is already `pending` or `unknown`, the journal stays unchanged and switching stays suppressed, because the earlier request may have reached the service.
2. It sends one authenticated request to the ChatGPT reset service used by Codex, with the active account header and the journaled idempotency key. No token, email, or response body is logged.
3. `reset` and `already_redeemed` are treated as idempotent success. `nothing_to_reset` and `no_credit` permit normal auto-switch fallback. A transport failure or unparsed HTTP response, including 4xx, retains the same key and suppresses switching until the result is settled.
4. Only after a confirmed success does the monitor use Desktop's existing owner-routed IPC recovery path to resume the blocked task(s).

The monitor never starts a second app-server and never asks another runtime to load the task. Desktop remains the only thread writer; the direct service call is limited to the account-level reset operation. Desktop does not need a custom reset IPC handler.

For an unresolved **manual** reset, inspect only `target_id`, `before_credits`,
`started_at`, and `state` in `~/.codex/manual-reset-state.json`; keep its
`idempotency_key` private. Confirm the exact account in the official ChatGPT
usage view and obtain authoritative evidence that the original request was
applied or was not consumed. An unchanged cached credit count alone does not
settle an unknown request. An official weekly window that began after the
journal's `started_at` can also close the old attempt. Until then, leave the
journal and do not issue another manual reset. After the outcome is settled and
no `cxi reset-account` process is running, the operator can remove only the
exact private journal path:

```sh
journal_path="${CODEX_HOME:-$HOME/.codex}/manual-reset-state.json"
if [ -f "$journal_path" ] && [ ! -L "$journal_path" ] &&
   [ "$(stat -f '%u:%Lp' "$journal_path")" = "$(id -u):600" ]; then
    rm -- "$journal_path"
fi
```

On a custom `CODEX_HOME`, use that same home for the official quota check and
the exact journal path. The manual state file is a small atomically replaced
document, so it stays uncompressed for reliable crash recovery.

For a pending or unknown **automatic** reset, inspect only `account_id`,
`episode_key`, `state`, and `updated_at` in `auto-reset-state.json`; keep its
`idempotency_key` private. A changed cached weekly timestamp or credit count
does not settle the request. Let the same-key retry reach a parsed terminal
result, or obtain official same-account evidence that the request was applied
or was not consumed. A verified later weekly window can also close the old
attempt. If the journal must be cleared after that proof, first disable the
weekly auto-reset option with `cxi config --auto-reset-weekly-enabled false`
and confirm no reset operation is running. Remove only the exact private path:

```sh
auto_journal_path="${CODEX_HOME:-$HOME/.codex}/auto-reset-state.json"
if [ -f "$auto_journal_path" ] && [ ! -L "$auto_journal_path" ] &&
   [ "$(stat -f '%u:%Lp' "$auto_journal_path")" = "$(id -u):600" ]; then
    rm -- "$auto_journal_path"
fi
```

The Monitor owns the launchd daemon lifecycle. Explicit Quit writes a private durable cancellation marker and unloads the recurring daemon. A scheduled worker that has not crossed the shutdown boundary stops; a worker already between shutdown and relaunch is allowed to restore Codex to a safe running state, but cannot begin another restart cycle. Starting Monitor clears the stale cancellation marker.

---

## 📁 Configuration Structure

The Monitor stores its account registry, status cache, and recovery journals in
`~/.codex/`, alongside files owned by official Codex Desktop:

### Monitor-owned (runtime removed by uninstall; account registry needs `--purge-data`)

- `~/.codex/accounts.json` — Stored multi-account credentials and cached quotas (strict `0600` permissions); removed only with `--purge-data`.
- `~/.codex/usage-status.json` — Real-time quota snapshot consumed by the macOS Menu Bar app; removed by the normal uninstall.
- `~/.codex/desktop-app-session.json` — Private APP account binding to the exact Desktop PID and birth identity, plus expected CLI account; an unbound or previous-process record is not display or recovery authority.
- `~/.codex/monitor.lock`, `daemon.lock`, `codex.lock` — Monitor coordination locks; removed when not held.
- `~/.codex/auto-reset-state.json`, `manual-reset-state.json`, `distribution-journal.json`, `direct-switch-journal.json`, `desktop-recovery.json`, `desktop-recovery.lock`, `desktop-automation-cooldown` — Private switching/recovery/reset state removed by uninstall. Interrupted staging files from Monitor writers are removed too. Journal, Desktop-session, `manual-reset-state.<pid>.<nonce>.tmp.json`, and the Monitor's `auth.json.<pid>.<nonce>.tmp` credential copy must match the exact name, your user, mode `0600`, and be a regular file (not a symlink or directory), so look-alikes are kept. Other Monitor staging names are removed by name prefix and suffix as regular, non-symlink files.
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
