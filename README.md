# 🚀 OpenAI Codex Multi-Account Monitor & Switcher

Ultra-lightweight, high-performance automatic quota monitoring and account rotation system for **OpenAI Codex** (supporting both console **Codex CLI** and desktop **Codex / ChatGPT.app**).

**Condition of use:** Register and switch only accounts that you own and are authorized to use. Recovery may reopen a selected task under another of your accounts; ChatGPT still controls whether that signed-in account can access the task.

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
   - Before any restart, it checks the exact ChatGPT process and WindowServer window inventory. For direct CLI restarts, multiple user windows require the explicit `--restore-window-tasks` flag. For distribution of a running Desktop, the Rust lifecycle captures every selected task before shutdown. An ambiguous inventory blocks either path before credentials change, even when window-bound preservation is disabled.
   - `cxi restart --restore-window-tasks` and `cxi switch <account> --restore-window-tasks` are explicit, per-invocation requests to bring every ChatGPT window back on the task it showed. ChatGPT itself persists only one window's bounds, relaunches one window without a task, and sends a `codex://threads/<id>` link to its most recently focused primary window; File > New Window opens a focused window (inspected in ChatGPT 26.924.22138). Before shutdown the window helper focuses each window, reads its task with Copy deeplink, and records its frame and focus. A window without a readable task (for example the home page or Settings), two windows on the same task, a minimized, full-screen, or ambiguous window, a missing File > New Window while more than one window is open, a changed window list, or a keymap change refuses the restart before credentials or checkpoints change. After the relaunch, once Desktop IPC answers, the helper reuses the relaunched window for one saved frame, waits up to 20 seconds for File > New Window (Desktop adds it only once its renderer is ready), opens each other window with it, sends each task link only while Accessibility shows its target window focused (focus leaving that window fails as `NAVIGATION_TARGET_CHANGED`), checks the windows it already restored before sending a second link, and counts a window only when it is on its frame and its own Copy deeplink returns the planned task. When recovery could have sent its own task link (a recovery target that no restored window showed, or an incomplete restore), including when recovery failed, a second pass moves back only a window that now shows a recovery task instead of its own, with the same link checks and a final pass after every attempted link, including when the target never verifies. It cannot tell recovery's link from yours, so a window you moved onto a recovery task during recovery is moved back too; other windows you changed, closed, or opened are left alone, and afterwards the app you were using is activated again. The restore plan with task IDs reaches the helper on stdin and the snapshot returns on stdout; neither is logged or written to a file, but each task link is handed to macOS `open` as recovery already does, so the ID appears briefly in that process's arguments. Each window-task helper run has a deadline (for a restore of two windows, 30 seconds plus 2 × 82) after which it is stopped with its whole process group, so an unanswered pasteboard prompt cannot hold the restart; the helper also stops on its own once the `cxi` that started it is gone. The clipboard is put back when no other app wrote to it after the last copy (a copy you make while it runs is what comes back; a late task link from ChatGPT is not mistaken for yours); when the snapshot or a restore could not put it back, the restart prints `WINDOW_TASKS_CLIPBOARD_NOT_RESTORED`. A restore failure after the relaunch, including a relaunch that never reached the restore, is reported with the restart result (every task still exists and can be reopened from the sidebar) and never changes recovery. Only a command whose `--trigger` is exactly `user` (the default) accepts the CLI flag. Distribution and auto-switch use their own Rust lifecycle session whenever they restart a running Desktop; an offline switch has no live windows to capture. A direct CLI switch that does not restart a running Desktop says the flag had no effect. A failed direct CLI restart after shutdown (checkpoint, account check, or launch) reports that its windows were not restored. After a post-stop distribution failure, a guarded relaunch of the previous account restores the captured window tasks before finishing the task session. If that relaunch or its IPC, account, process, or session checks fail, the result reports incomplete window restoration. It needs the probe's preconditions below, English Desktop menus, and Desktop's New Window feature. **This path has not yet run against a live Desktop**: run the rehearsal below on your windows first.
   - A post-recovery recheck reports a window as unverified when its saved frame is absent or ambiguous, or when Copy deeplink cannot read its task. It leaves that window alone and reports incomplete restoration rather than claiming success. A rehearsal compares its final WindowServer/Accessibility window IDs with the exact original IDs; an extra window or unreadable final inventory reports `REHEARSAL_WINDOW_LEFT_OPEN`. A failed window-list read after New Window also attempts to close a known focused new window.
   - After any task-link attempt, the recheck reads previously verified windows again, including when the attempted target did not load. A late link can land in an earlier window without changing keyboard focus; that earlier window then counts as unverified.
   - `cxi window rehearse-task-restore --allow-focus-and-clipboard` rehearses those restore steps without a restart and with the probe's preconditions: it reads each window's task, opens one new window per original with File > New Window at an offset frame, sends it the original's task link, requires its Copy deeplink to return that task, requires every original window to still show its own task (navigating an original back before reporting `ORIGINAL_WINDOW_CHANGED`), then gives each window it opened its opening frame back, closes only those windows, refocuses the original window, and puts the clipboard back. It prints only counts or a fixed failure code (`REHEARSAL_WINDOW_LEFT_OPEN` if a window it opened could not be closed or closure could not be verified). It proves focus-targeted navigation and New Window with tasks that are already loaded; it cannot show how a cold task loads after a relaunch, when New Window appears after a relaunch, or how the relaunched window is reused.
   - `cxi window probe-tasks --allow-focus-and-clipboard` is an explicit diagnostic for the window-to-task mapping; it proves nothing about restart or restoration and has not been run against a live multiwindow Desktop. Before any visible change it requires the flag; a Codex home that resolves to the account's `~/.codex` (from the user database, not `$HOME`; a ChatGPT started with its own `CODEX_HOME` is not detected); the switch/recovery operation lock, held for the whole run (a switch or recovery already running refuses the probe, and one started during it is refused); a ChatGPT keymap (`~/.codex/keybindings.json`) that is absent, blank, or an array of exactly `{command, key}` entries none of which names `copyDeeplink` or puts any key combination on L (ChatGPT's own rules then keep Cmd+Opt+L on Copy deeplink; an unrelated override such as a dictation hotkey is accepted); exactly one ChatGPT process; ChatGPT 26.924.22138 (build 11645), the only build whose Copy deeplink binding, keymap rules, link routing, and New Window command were inspected, with its bundle unchanged since launch; no macOS App Shortcut on Cmd+Opt+L; a keyboard layout on which that key types `l` with Command held (not Dvorak or Colemak); Accessibility and event-posting access; pasteboard access not set to deny (macOS 15.4 and later); and no minimized ChatGPT window. It then focuses each ChatGPT window, sends Cmd+Opt+L only to that ChatGPT process, and reads the copied link. Each copy requires keyboard focus on that window observed through Accessibility, an unchanged process birth, unambiguous AX/WindowServer geometry, a stable window inventory and mapping, exactly one clipboard write that stays unchanged while it is read, no concealed or transient pasteboard marker, and a unique canonical task link. On macOS 15.4 and later, macOS may ask before the helper reads the pasteboard; a prompt that takes focus makes the probe fail closed, after which the app that ran it can be set to always allow pasteboard access in System Settings and the probe rerun. The helper never outputs task IDs, and the CLI prints only a window count or a fixed failure code; a failure that may have followed a focus change says so. ChatGPT itself puts each link on the system clipboard, so other apps, clipboard history, and Universal Clipboard can see it. The helper keeps the previous clipboard items in memory (and the newer ones if another app writes between its copies) and writes them back when the change count still matches its last copy; private (concealed or transient) contents are never read or restored, contents found to exceed 32 MiB are not kept, and a write by another app after the last copy is kept, so the last link may stay. A single competing clipboard write of a valid task link remains indistinguishable, so attribution is unverified. A keymap edit still present when the probe ends voids the result. The probe saves no restart snapshot and does not relax the multiwindow guard. `Task probe failed: COMMAND_REJECTED` means the installed window helper predates the probe; rerun the installer. Missing WindowServer window titles fail closed as `WINDOW_INVENTORY_MISMATCH`.
   - Window inventory cross-checks named WindowServer windows against Accessibility standard-window frames. An unnamed offscreen window is excluded only if its frame is distinct from every standard-window frame; an unexpected or malformed offscreen title blocks shutdown. This preserves the observed single-window Desktop with a detached renderer, but the completeness of Accessibility's window roster is still unproven.
   - The shutdown inventory checks Accessibility and Screen Recording authorization without prompting. `WINDOW_ACCESSIBILITY_DENIED` and `WINDOW_SCREEN_RECORDING_DENIED` identify missing grants before any Desktop signal. This guard runs for automatic distribution even when window-bound preservation is off. A Terminal grant does not prove that launchd `cxi` has the same access.
   - The Monitor binds the displayed APP account to the exact relaunched process before waiting for task recovery.
   - Keep automatic switching disabled until unattended cold-task mounting and exact selected-task restoration across windows are proven on the installed Desktop.
   - If Desktop has a verified zero-window inventory before a restart, automatic switching can continue without task or geometry restore; a newly opened window then blocks shutdown. If there are recovery targets, owner-routed IPC waits for a visible banner after owner mounting; a missing window then defers the target with its original checkpoint. Accessibility failures, malformed geometry, and process identity mismatches still stop a preservation-enabled switch before credentials change.

6. **Automated Session & Thread Resumption Across Switches**:
   - Detects eligible mid-turn tasks captured for a restart and threads whose latest quota-error `task_complete` rollout event occurred within the last 4 hours (`RECENT_QUOTA_WINDOW_SECS = 14400s`); discovery-only recovery does not guess about an ambiguous active turn.
   - Scans up to 30 recent threads ordered by `state_5.sqlite` `updated_at`, then checks quota age from the rollout event timestamp. Bounded 128 KB tail reads (`read_rollout_tail_lines`) avoid reading entire large session files during discovery.
   - Resumes through the official Codex Desktop owner's IPC connection to the Desktop-bundled app-server; it never launches a second/headless app-server, uses `codex exec resume`, or clicks UI controls. For an interrupted turn it sends one protocol-valid text input, `continue`, through `thread-follower-start-turn`.
   - Shows a verified semi-transparent banner when an eligible window and recovery target are present, including when exact window restoration is disabled. When there are zero running recovery targets, a captured selected window instead gets a generic one-window restart panel with no task rows or resume claim. Requires a new exact-ID `task_started`, real agent work, and a 10-second error-free observation window before reporting task recovery success.
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
   - `Launch at Login` toggle that shows the login item macOS reports, read again each time the menu opens: checked only when the main-app login service or a System Events login item (such as the one `scripts/install.sh` adds) opens this app, a dash when System Events cannot be read, and a warning when a change does not take effect. macOS may ask once whether Codex Monitor may control System Events; it only reads and changes its own login item.

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
   Run `./scripts/test_swift.sh` before installing. The Swift tests never
   read or create your `~/.codex` (or `CODEX_HOME`): each builds its Monitor
   client on a temporary Codex home. If Swift reports
   `this SDK is not supported by the compiler`, read the error printed before
   it first: an unwritable module cache produces the same message, so set
   `CLANG_MODULE_CACHE_PATH` to a writable directory. Otherwise repair or select
   matching Command Line Tools and rerun the test. Do not publish or reinstall a
   partially built app bundle.
   A module cache works only at the exact path that built it: reusing it
   through another spelling of its directory (`/tmp/x` for `/private/tmp/x`)
   or after copying or moving it fails to compile, with an error that depends
   on the Swift version. So if you set `CLANG_MODULE_CACHE_PATH`, the test and
   install scripts create the directory if it is missing, resolve it to its
   physical path (a relative path from where you ran them), and compile into
   their own subdirectory of it, `codex-monitor-swift-<checksum of that
   path>`. Any spelling of the directory reaches the same subdirectory, a copy
   or move starts a new one, and the rest of the directory is left to your
   other tools. Delete old `codex-monitor-swift-*` directories to reclaim
   space. The scripts stop if the path is not a directory, contains a newline,
   or starts with an unexpanded `~`, or if the subdirectory is not a plain
   directory that you own, that they can write, and that group and others
   cannot write. Other Swift or Clang runs that share a cache among
   themselves must use one spelling of it, and a new directory after a copy or
   move.
3. **Rust & Cargo** (for building the ultra-lightweight CLI core):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   The build uses the exact toolchain pinned in `codex-switcher/rust-toolchain.toml`;
   the installer asks rustup to install it before compiling. It builds the crate
   versions in the committed `codex-switcher/Cargo.lock` with `--locked` and stops
   if that lockfile is missing or out of date.

### 🦀 Rust toolchain policy

`codex-switcher/rust-toolchain.toml` pins one exact Rust release (with Clippy
and rustfmt) so a new upstream `stable` cannot change build or lint results
without a reviewed commit. rustup applies the pin whenever cargo runs from inside
`codex-switcher/`, which is how the installer, the shell tests, and CI invoke it.
It does not apply to `cargo --manifest-path` run from another directory, or to a
Rust installed without rustup (the installer warns in that case).

CI installs the pinned toolchain with `rustup toolchain install`, then runs
`cargo clippy --workspace --all-targets --locked -- -D warnings` (every target,
tests included) and `cargo test --locked` against the committed `Cargo.lock`.

`codex-switcher/tests/ci_workflow_policy.rs` enforces the policy: it rejects
floating channels and per-command overrides in workflows, requires that exact
Clippy step in a required Rust job, rejects inherited variables, default
shells, and `GITHUB_ENV`/`GITHUB_PATH` references that could weaken it, and on
GitHub Actions asserts that the tests themselves ran under the pinned release.

To bump the toolchain:

1. Change `channel` in `codex-switcher/rust-toolchain.toml` to the new exact `X.Y.Z` release.
2. In `codex-switcher/`, run `rustup toolchain install`, then `cargo test --locked` and
   `cargo clippy --workspace --all-targets --locked -- -D warnings` on the new
   toolchain; fix every lint the new release adds in the same PR, because the
   CI Clippy gate fails on any finding.
3. Open a dedicated `build/` PR and confirm every required check passes on its head SHA.

GitHub Actions in `.github/workflows/` are likewise pinned to full commit SHAs
with a `# vX.Y.Z` comment. To update one, resolve the release tag to its commit
(`gh api repos/<owner>/<action>/commits/<tag> --jq .sha`), confirm the tag points
at that commit, and update both the SHA and the comment.

### 🔒 Dependency lockfile policy

`codex-switcher/Cargo.lock` is committed as a reviewed supply-chain input. The
installer, the shell tests, and CI build with `--locked`, so cargo fails when the
lockfile is missing or would change instead of silently resolving whatever
crates.io serves that day. Use the same flag locally in `codex-switcher/`:

```bash
cargo test --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`cargo fmt` does not resolve dependencies and takes no such flag.
`codex-switcher/tests/ci_workflow_policy.rs` enforces the policy: every cargo
command in `.github/workflows/` and in tracked `*.sh` scripts passes `--locked`
(or `--frozen`) before any bare `--`, the lockfile is committed and matched by no
`.gitignore` rule, every `hashFiles()` cache-key input is a committed file, and
on GitHub Actions the lockfile still matches `HEAD` when the tests run. The scan
is conservative: it reads a `$CARGO`-style command word or `$(command -v cargo)`
as cargo, reports a quote left open at the end of a file, and treats a bare
lowercase `cargo` word in prose (an `echo` message, a heredoc) as a command, so
word such messages differently.

To change dependencies:

1. After editing `Cargo.toml`, run `cargo check` once without `--locked`; cargo
   adds the new entries and keeps existing versions unless a new requirement
   needs a newer one. To move existing crates, update only what you mean to,
   such as `cargo update -p <crate>` or
   `cargo update -p <crate> --precise <version>` (plain `cargo update` moves
   every crate to its newest compatible release).
2. Review the `Cargo.lock` diff: new crates, version jumps, and any `source`
   other than `registry+https://github.com/rust-lang/crates.io-index`.
3. Run `cargo test --locked`, then commit `Cargo.toml` and `Cargo.lock` together
   in a dedicated `build/` PR.

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
The attempt file and its directory entry are flushed before the request. If
saving the attempt or reading it back fails, no request is sent, and the
command withdraws (marks resolved) only a record that reads back exactly as
the attempt it just created, so an unsent attempt cannot block every later
manual and automatic reset. An unreadable or different record may belong to
another operation and is left for reconciliation.
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
1. **Compiles the Rust CLI (`codex-mon` / `cxi`)** with maximum release optimizations, from the dependency versions pinned in the committed `Cargo.lock` (`cargo build --release --locked`).
2. **Installs to `~/.local/bin`** and adds an exact shell `$PATH` block to `~/.zshrc` (and an existing `~/.bash_profile`).
3. **Configures the transparent CLI shim** at `~/.local/bin/codex`.
4. **Builds the native macOS Menu Bar application** (`Codex Monitor.app`) and installs it into `/Applications` when writable, otherwise `~/Applications` (a system install also creates a `~/Applications` symlink).
5. **Registers macOS Login Item** for seamless launch on Mac startup.
6. **Configures 24/7 background `launchd` daemon** (`~/Library/LaunchAgents/com.codex.switcher.plist`).
7. **Integrates AI Agent Skills** into `~/.codex/skills`, `~/.claude/skills`, and `~/.agents/skills`; a one-line remote install retains their source under `~/.local/share/codex-monitor/skills` because its temporary clone is removed.
8. **Builds the optional `Codex Notifier.app`** in `~/Applications`, the `codex-ui-resume`, `codex-recovery-banner`, and `codex-window-restore` helpers in `~/.local/bin`, and bundles the confirmation-gated uninstaller inside the Monitor app.
9. **Launches the Menu Bar app immediately.**

For stable background `cxi` and `codex-window-restore` identities across rebuilds, an operator who
already has a valid code-signing identity may set
`CODEX_MONITOR_SIGNING_IDENTITY_SHA1` to its 40-character fingerprint when
running the installer. The installer checks that identity, signs the staged CLI
and window helper with fixed identifiers `com.codex.monitor.cli` and
`com.codex.monitor.window-restore`, and refuses an invalid selection before
building. A Developer ID Application identity is recommended for distribution.
With no selection, both remain ad hoc signed; macOS privacy grants may need
renewal after an update. Reuse the same identity on every install and verify
both installed designated requirements and launchd-origin grants afterwards.
A local self-signed certificate may yield a
stable requirement on one Mac, but its Accessibility and Screen Recording grant
behavior is unverified. The installer neither creates nor trusts certificates.

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
`Codex Notifier.app`, `cxi`/`codex-mon`/`codex` and the `codex-ui-resume`,
`codex-recovery-banner`, and `codex-window-restore` helpers, the
`com.codex.switcher` LaunchAgent and restart worker, the Monitor Login Item,
Monitor preferences, app-specific support/cache/saved-state directories, the
copied help file, Monitor logs/journals, the remote-install skill cache, and
skill links that point to this project. The script is idempotent and does not remove
anything merely because it happens to be named `codex` unless it is the exact
shim installed by this project.

While an installation runs, the uninstaller changes nothing: it prints that an
installer holds the install lock and exits with status 75, so rerun it after
the installation ends. The Menu Bar app's Uninstall item has already quit the
app by then and shows nothing, so if the app is still installed afterwards,
run `scripts/uninstall.sh` from a terminal once the installation is done (a
successful installation reopens the app). The lock is
`codex_monitor_install_<uid>.lock` in the per-user temporary directory
(`getconf DARWIN_USER_TEMP_DIR`), whatever `TMPDIR` says, so an uninstall
started from the menu finds an installer started in a terminal. A lock that
an older installer kept in the uninstaller's own `$TMPDIR` or in `/tmp` counts
too. The installer takes the lock with `lockf` (macOS 15 and later) or perl's
`flock` (macOS 13 and 14 ship no `lockf`), and refuses to run when it can take
neither or cannot find that directory. A program the installer starts that
keeps running after it (a compiler cache server, for example) does not keep
the lock. The lock ends within a fraction of a second of the installer's
exit, so an uninstall started at that very moment may still be refused; run
it again.

It also removes what a killed installer or uninstaller left behind. The
installer creates its temporary paths only while it holds its install lock
and removes them before releasing it:

- `~/.local/bin/.codex-mon.install.XXXXXX` (the CLI being installed, mode
  `0600` or `0755`) and codesign's `.codex-mon.install.XXXXXX.cstemp` copy
  (mode `0755`);
- `.codex-monitor-install.XXXXXX` and `.codex-monitor-backup.XXXXXX` in
  `/Applications` or `~/Applications` (mode `0700`, empty or holding only
  `Codex Monitor.app`);
- a one-line remote install's clone,
  `codex-mon-install-XXXXXX.XXXXXXXXXX` in the per-user temporary directory
  (mode `0700`; eight characters after the dot on macOS 13, ten later).

Each `X` is a letter or digit, as `mktemp` fills it. The name, your user, the
type and mode must all match, and nothing is followed through a symlink.
While an installation runs, a backup directory can hold the only copy of the
previous app. The uninstaller therefore removes these only when no installer
holds a lock, checked again just before removal. If one does, or a lock
cannot be verified (for example a symlink in its place), it keeps them,
names them, and finishes with a warning. Once the installer has exited, a
backup is leftover Monitor data and is removed like the app, including one
the installer kept because it could not roll back. The uninstaller also
removes the private copies it edits in place of `~/.zshrc`,
`~/.bash_profile`, and `~/.codex/config.toml`
(`<file>.codex-monitor-uninstall.XXXXXX`) if an earlier run was interrupted.

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
not purged by default. Compiler build caches are not removed either: the default
Clang module cache, and the `codex-monitor-swift-*` subdirectory created in a
`CLANG_MODULE_CACHE_PATH` you set before installing.

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
# Bring every ChatGPT window back on its task (focuses windows, uses the clipboard):
cxi switch backup --restore-window-tasks
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
# The same, reopening each ChatGPT window's task (rehearse it first):
cxi window rehearse-task-restore --allow-focus-and-clipboard
cxi restart --restore-window-tasks
```

Self-restart is handed to an independent one-shot launchd worker. A failed
underlying recovery is recorded as `WORKER_RESULT failed`, while the wrapper
exits cleanly so launchd cannot repeat the destructive restart. `RESTART_DISPATCHED`
means scheduled, not completed; the printed log records old/new app PIDs and
per-task `RECOVERY_VERIFIED` or `RECOVERY_FAILED` outcomes. Distribution logs
`RECOVERY_NOT_REQUESTED` when it captured no eligible recovery targets and
verified only Desktop stability. Active logs remain
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
| Banner minimum visibility | `5 s` | Keeps the semi-transparent recovery banner visible when an eligible window was captured, including the zero-target window-only panel. |
| Automatic distribution backoff | after `2` identical pre-signal failures: `5 min`, doubling to at most `30 min` | Daemon only, in memory. Holds back the same automatic plan (cause and accounts) after it failed the same way before Desktop was signalled, so a doomed switch does not repeat its thread scan every tick. Manual switches ignore it. |

An ownerless deferred mount shows a pending banner while ChatGPT opens the task.
The banner describes verification in progress; it does not promise a restart or
continued work before a Desktop owner and recovery are confirmed. Distribution
and direct switching check the target Desktop account and session both before
and after window restoration, immediately before recovery IPC. A confirmed
account-switch auth error is eligible for this mount only while its saved
rollout interval and queue snapshot remain current.

While that hold is active, the daemon's two-second depleted-account watchdog
does not wake another full tick. An enabled weekly auto-reset can still wake a
full tick for a recent blocked task, at most once every 30 seconds. The
configured polling interval and auth-file-change wakeup still apply, and
deferred task recovery continues its lightweight polling.

The tail reader checks the byte before its seek point. It discards a partial
first record before strict UTF-8 decoding, retains a full record at an exact
newline boundary, and reports unknown state for malformed complete records.

`launchd` can deny Accessibility and Screen Recording to the window helper even
when the helper succeeds from Terminal. Before every restart, the shutdown
guard counts ChatGPT's standard windows through Accessibility and cross-checks
them against WindowServer. It checks both grants before any credential change
or Desktop signal; disabling window bounds preservation does not bypass it.
`WINDOW_ACCESSIBILITY_DENIED` and `WINDOW_SCREEN_RECORDING_DENIED` name the
missing grant without opening a macOS prompt. A distribution may first call
the helper while capturing selected window tasks, yielding
`phase=WINDOW_TASK_CAPTURE_FAILED`; a later shutdown preflight yields
`phase=SHUTDOWN_WINDOW_GUARD_FAILED`. The outcome includes the matching
`pre_signal_phase`. Both stop before Desktop is signalled.

On macOS 27.2, find Accessibility in System Settings under Privacy & Security >
Device Control and Data Access. Screen access is under Screen & System Audio
Recording. Grant each denied service to the exact current executable macOS
attributes the background request to, potentially both `codex-mon` (`cxi`) and
`codex-window-restore`, then verify a fresh launchd-origin window inventory.
An enabled row in System Settings does not prove that a rebuilt ad hoc signed
binary is trusted: on this host both rows were enabled under Device Control,
yet a launchd helper run returned `WINDOW_ACCESSIBILITY_DENIED` after reinstall.
The installer never grants either permission. A successful grant still does not
prove selected-task restoration across multiple windows in the installed app.

Every pre-signal failure (`WINDOW_CAPTURE_FAILED`, `WINDOW_TASK_CAPTURE_FAILED`,
`SHUTDOWN_WINDOW_GUARD_FAILED`, `RECOVERY_CHECKPOINT_FAILED`,
`RECOVERY_PREFLIGHT_FAILED`, or a `SHUTDOWN_FAILED` rejected before the signal)
leaves credentials and Desktop unchanged. When the daemon's same automatic plan
fails twice in a row with the same phase and message, it logs
`AUTO_BACKOFF_ARMED` and defers that plan (status `deferred_cooldown`, with one
`AUTO_BACKOFF_ACTIVE` line per hold) for 5 minutes, doubling with each further
identical failure up to 30 minutes. A plan with a different cause, target, or
current account is not held. The streak ends when an attempt of the plan has
any other result (success, or a failure after Desktop was signalled), when a
different plan or failure is recorded (only the latest plan is remembered),
when an hour passes without a failure, or when the daemon restarts. Ticks that
never reach an attempt (no action needed, cooldown, an in-flight journal) do
not change it. Changing settings does not lift a hold; run `cxi switch` or
restart the daemon to retry sooner. The hold is measured in awake time, so it
pauses while the Mac sleeps. `cxi switch` and the Menu Bar app are never held
back. The `codex` wrapper's preflight runs in its own process and is not
covered.

### 🚦 Rollout Lifecycle States (`ThreadRolloutState`)

- **`InterruptedByQuota`**: The turn's final `task_complete` contains an `error` payload matching `usage_limit_exceeded`, `workspace_owner_credits_depleted`, `out of credits`, or active `rate_limit_reached_type`. **Automatically resumed.**
- **`ActiveInProgress`**: The latest event is a mid-turn event (`user_message`, `reasoning`, `custom_tool_call`, etc.) with no closing `task_complete`. A captured restart may resume it from its post-shutdown checkpoint. Discovery-only recovery refuses this ambiguous state so it cannot duplicate or stop a task the user already resumed.
- **`InterruptedByError`**: The final `task_complete` carries a non-quota error (such as a 401 auth outage or a policy block) and no non-empty `last_agent_message`. Ordinarily only explicit `cxi resume <id>` may continue it. A narrow exception applies to the exact token-refresh error caused while Monitor changes between two verified Desktop accounts: the task must have been active at the first pre-stop checkpoint; the same rollout file and turn must end with that exact error between the first and second checkpoints, with no Stop or new user turn. The saved rollout file identity, length, and exact terminal interval, plus the queue database identity, revision, and pending count, are rechecked before deferred mounting and immediately before dispatch. The saved target account must match the verified relaunched Desktop before owner-routed IPC. This evidence is durable for a deferred cold-task mount. Old auth-error chats without that operation evidence remain explicit-only. An error after a final agent message, including a non-null non-string final field, is treated as completed.
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

1. Detect eligible, unarchived non-subagent tasks and atomically journal their IDs before shutdown. Stale manifest IDs and a caller-provided primary task are revalidated against SQLite and can never force an internal subagent into recovery. Duplicate task IDs in the recovery journal fail validation. When recovery targets or captured selected-window tasks exist, handshake Desktop IPC before stopping ChatGPT; failure restores the prior checkpoint and leaves Desktop running.
2. Gracefully stop Desktop, wait for the exact main process to exit, then record a second rollout checkpoint. This excludes old work and shutdown-flush events from recovery proof. A deferred entry for the same task is replaced only after a newer turn has substantive, error-free work and no queued follow-up; unrelated deferred entries keep their account binding. The evidence scan reads a fixed rollout snapshot with bounded memory, including long turns. If the second checkpoint fails, previous shared authentication is staged for relaunch and its new process is bound to the same account; recovery requests are not dispatched. `cxi restart` also relaunches the previous Desktop state before reporting the error.
   For a verified account change, a bounded streaming scan of the interval between checkpoints can confirm the exact token-refresh failure of the previously active turn. Unreadable, changed, oversized, or ambiguous records grant no automatic error recovery.
3. Relaunch Desktop, validate its same-user IPC socket, and resolve the owner of every task. While Desktop is already running, each task is routed to its own owner, including separate windows under one account. A direct CLI restart with multiple windows requires `--restore-window-tasks`. Running-Desktop distribution captures every window task internally, including Menu Bar and automatic triggers, and requires the exact window set and unchanged task snapshot before shutdown. After relaunch it restores each selected task before recovery and rechecks windows after recovery with eligible targets, including recovery failures. Target auth, session marker, PID, and birth are checked after IPC readiness and after the helper; a small interval for an independent Desktop change remains. Only ownerless cold tasks open a task URL; later `open -g -a /Applications/ChatGPT.app` retries continue, with one pinned native LaunchServices attempt after at least 10 seconds of initial ownerless waiting. Foreground activation is allowed. Already-owned tasks are never cycled through the UI. URL acceptance does not prove mounting: owner discovery remains mandatory before dispatch. Every recovery mode pins the uniquely verified active account and exact Desktop PID and birth identity at operation start, then rechecks both after owner discovery before and after durable checkpoint consumption. A pre-send mismatch restores the original checkpoint with readback and sends no IPC. Finalization preserves an explicitly claimed deferred target's original owner account and offset; a previously unbound target retains its offset but binds to the account that started recovery, blocking an unattended retry under the changed account. A hidden banner from a captured restart binds the relaunched process only through the exact live Desktop session marker.
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
evident. In 26.924.22138 that window is the most recently focused primary
window, and no link parameter selects or creates a window. No recovery turn was dispatched in those checks. The switcher
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
bounded owner waits. A later deferred navigation attempt for a same-account,
eligible ownerless task holds a `Pending` banner during its bounded mount wait
once a Desktop window is visible, then passes the same panel into recovery after
owner proof. A missing window, timeout, account or process change, or exited
helper retains the checkpoint without IPC. It rechecks the
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
unattended retry intent unless it carries confirmed operation-bound auth-rotation
evidence for the target Desktop account.
Malformed or changed tail state keeps the original intent.

### ♻️ Account-Bound Weekly Reset Credits

The weekly reset option is off by default and is intentionally independent of account rotation. When enabled, the daemon acts only when all of the following are true: the active account's fresh weekly availability is exactly `0%`, a reset credit is available, the selected strict remaining-time threshold is met, and a recent (up to four hours), unarchived, user-owned task ended with a quota error. It never spends a credit for an idle account, an active task, an aborted task, or a subagent.

The monitor writes a private, atomic `~/.codex/auto-reset-state.json` journal before requesting a reset, while holding the recovery operation lock. It contains an opaque idempotency key, account/window marker, and task ID, is mode `0600`, and is deliberately not a log. A timeout or unknown result is retried only with that same key when the original episode remains eligible; a successfully applied reset is never consumed again for the same weekly window. A corrected weekly timestamp or a different active account does not authorize replacing the sole unresolved journal. A restored quota snapshot cannot silently discard an unresolved attempt.

The reset is account-scoped and does not participate in thread ownership:

1. After taking the same operation lock as account switching, the monitor reloads the active account and revalidates its exact weekly exhaustion and window marker, selected threshold, reset-credit count, account routing ID, live CLI authentication, that a request can be built for the account route and token, and running Desktop. Only then does it write the `pending` journal entry, and the request follows immediately, so a change noticed during preparation never leaves an unsent attempt marked unresolved. The journal file and its directory entry are flushed before the request. If that write fails, nothing is sent: a `pending` entry that reads back exactly as this write is withdrawn to a retryable `journal_error` (reason `pending_marker_not_durable`) that keeps its key and task, and an unreadable or different journal is left unchanged. If a check fails, or the request cannot be built, while retrying an attempt that is already `pending` or `unknown`, the journal stays byte-for-byte unchanged and switching stays suppressed, because the earlier request may have reached the service. The status reason then names why this retry waits (`retry_refused:<cause>`, `retry_unavailable:<reason>`, or `original_reset_task_is_no_longer_quota_blocked`); the journal keeps the attempt's own reason.
2. It sends one authenticated request to the ChatGPT reset service used by Codex, with the active account header and the journaled idempotency key. No token, email, or response body is logged.
3. `reset` and `already_redeemed` are treated as idempotent success. `nothing_to_reset` and `no_credit` permit normal auto-switch fallback. A transport failure or unparsed HTTP response, including 4xx, retains the same key and suppresses switching until the result is settled.
4. Only after a confirmed success does the monitor record `applied`, read fresh usage, and use Desktop's existing owner-routed IPC recovery path to resume the blocked task(s). The hand-off does not depend on saving `applied`, because no later tick repeats it: a visible `applied` returns early, and a `pending` entry that `applied` never replaced is held once fresh quota shows the restored pool. A failed `applied` write is repeated after the hand-off; if that also fails, the error says the credit was applied, and the `pending` entry waits for reconciliation.

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

Older builds could leave an attempt `pending` although its request was never
sent: a manual attempt whose save or readback failed after it was written, or
(before PR #20) an automatic attempt whose final checks refused after the
journal was written. The Monitor withdraws an unsent attempt only in the
command that created it, while its fresh key proves ownership; a record left
by an earlier run is indistinguishable from one whose request reached the
service, so it is never cleared automatically. Reconcile such a record with
the same evidence as any other unresolved attempt. An automatic one can also
settle itself through the same-key retry while its original task and weekly
window remain eligible.

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
