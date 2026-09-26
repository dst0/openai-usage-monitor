# OpenAI Codex Configuration & Integration

- Project: OpenAI Codex Usage Monitor & Account Switcher
- CLI Commands: `cxi` / `codex-mon`
- CLI Wrapper / Shim: `~/.local/bin/codex` -> `cxi wrap`
- Quota API Endpoint: `https://chatgpt.com/backend-api/wham/usage`
- OAuth Refresh Endpoint: `https://auth.openai.com/oauth/token` (Client ID: `app_EMoamEEZ73f0CkXaXp7hrann`)
- Native App: `/Applications/Codex Monitor.app` when writable, otherwise `~/Applications/Codex Monitor.app` (the installer may also leave a symlink at the latter path).

## Official Codex Desktop Integration

**Condition of use:** Every ChatGPT account configured in this Monitor belongs
to the same human owner of this device. Continuing that owner's local Codex
tasks after switching between those accounts is the intended product behavior.

The supported Desktop setup is the normal OpenAI Codex Desktop installation
(`/Applications/ChatGPT.app` in the standard macOS layout), signed in and
running normally. The installer does not patch `ChatGPT.app`, install a second
App Server, add custom App Server flags, or require manual IPC configuration.
The CLI shim and isolated account login resolve the executable inside that
bundle. Current builds use `Contents/Resources/codex-cli/bin/codex`; older
builds used `Contents/Resources/codex`. CLI-only setups may use a standalone
`codex` on PATH after excluding the Monitor shim and the path being replaced.
Installation preserves an existing standalone executable when that path is its
only CLI entry point. A missing real executable is an error before automatic
account distribution.

ChatGPT Desktop starts its bundled `codex app-server`. The Monitor is only a
same-user IPC client: it connects to `~/.codex/ipc/ipc.sock`, discovers the
Desktop window that owns a task, and routes `thread-follower-start-turn` or
queued-follow-up requests to that owner. The Desktop-owned app-server remains
the only thread writer; the Monitor never runs `codex exec resume`.
Owner discovery runs separately for each task, including when one account has
tasks open in several ChatGPT windows. The single-process restart check counts
ChatGPT's main processes, not its windows; recovery does not assume that the
frontmost window owns every task.

Without Desktop, CLI quota inspection and account switching still work, but
Desktop restart and thread recovery are unavailable. Optional read-only check:
`cxi recovery-preflight`.

Cold tasks may fail owner discovery after an accepted macOS deep link even
when Desktop IPC itself is healthy. The monitor now activates ChatGPT once
for an ownerless task and reissues later links in the background; it still
requires a real owner before dispatch. In that case automatic distribution reports
partial recovery rather than sending a turn without an owner. The daemon
retains only pre-dispatch tasks without a verified owner, including transient
URL-launch and Desktop IPC startup failures. It probes every 15 seconds and
reissues an ownerless task URL in the background at most once per minute. Once
ChatGPT mounts the task, it retries using the original checkpoint and normal
turn verification. For a deferred retry after App/CLI distribution, the saved
Desktop account must match the target, the exact live ChatGPT PID and birth
identity, and the expected CLI account; an old or unbound session cannot
authorize dispatch. The initial recovery banner closes after its bounded
owner waits; a new banner is required before any later owner-routed IPC send.
An older ownerless checkpoint stays eligible across another switch only while
the task has no new turn. If a later `task_started` appears after that
checkpoint, the new restart records a fresh offset and clears the old owner
account binding. A separate `task_started` followed by substantive agent work
retires the old retry when no queued follow-up remains, even when the user
resumed the task manually. An existing queued follow-up keeps the retry
eligible; metadata or a start without work does not count as completed recovery.
The evidence scan covers the complete post-checkpoint interval up to a captured
rollout length, using fixed-size read buffers and a bounded line buffer. Large
legitimate turns do not lose their retry solely because they exceed a byte
cutoff. Recovery manifests with duplicate task IDs fail validation on both
read and write, so two entries cannot claim conflicting account bindings.
The deferred worker keeps a bounded append cursor for a rollout it already
scanned, so later probes read newly appended bytes instead of replaying the
whole interval. File replacement, truncation, or changed identity resets that
cursor; account and Desktop-owner checks remain fresh on every probe. Ordinary
thread detection excludes ownerless entries from `load_pending()` because the
deferred worker owns those retries. An older interval is scanned in chunks of
at most 16 MiB per probe; partial scans return no lifecycle result until the
captured snapshot end is reached.
There is no persistent banner while Desktop has not mounted the task. Unattended mounting
after an account switch remains unverified on the current Desktop build. If the daemon is not
running, inspect the affected task and use
`cxi resume <id>` only if the turn remains interrupted.

An account switch records its recovery targets before shutdown and takes a
second checkpoint after the old ChatGPT process exits. The later offset keeps
shutdown-flushed events out of proof for the replacement turn. If that second
checkpoint cannot be saved, account switching stops before changing
`auth.json` and relaunches the previous Desktop account; a successful shutdown
alone is not permission to rotate credentials. `cxi restart` also relaunches
the previous Desktop state if its post-shutdown checkpoint fails.

The current Desktop interfaces do not give Monitor a stable mapping from each
window to its selected task, nor a way to reopen a specific task in a specific
new window. A WindowServer count based on window title and geometry was
rejected because it can classify a real user window as a hidden renderer.
Automatic switching stays disabled until multiple-window recovery can be
verified against the live Desktop.

Automatic distribution records whether the exact Desktop process has an eligible
standard window before shutdown. If no such window exists, it skips geometry
restore; after task owner mounting, recovery requires a visible banner before
IPC and retains the original checkpoint if no window appears. A launchd daemon may be denied Accessibility access even
when the same helper succeeds from Terminal. If that happens, set
`cxi config --preserve-window-bounds false` to explicitly disable geometry
preservation. Distribution validates the exact Desktop process without an
Accessibility window read, uses read-only WindowServer geometry to place the
banner when a visible window and recovery target exist, and still performs IPC
recovery and singleton-process verification. Explicit WindowServer visibility/geometry
failures and panel visibility failures are logged without blocking the account
switch. Helper protocol and unknown capture failures block the switch. Window access, geometry, and process-identity failures remain
blocking while preservation is enabled; process-identity failures remain
blocking in either mode. With preservation disabled, the prior window position
and size are not restored or verified by the Monitor.

Deferred recovery in an already running ChatGPT never restores window bounds.
It locates its banner through the read-only WindowServer helper, so a launchd
Accessibility denial cannot stop IPC recovery when WindowServer can place the
panel. An initial `WINDOW_NOT_FOUND` is retried after Desktop confirms the owner;
IPC requires a live panel at that point. The queue and rollout are rechecked
after panel startup, followed by a second helper/identity check after SQLite
waits and a final queue/rollout check before the durable dispatch marker.
Failed status replay into a late banner blocks dispatch. Window access/geometry
failure, panel timeout, process identity, missing helper, payload/lease failure,
and malformed helper output block dispatch and retain the original checkpoint.
Automatic switching stays disabled until a
quota-interrupted cold task completes end-to-end recovery in the installed app.

The Menu Bar's APP quota comes from `desktop-app-session.json` only when its
saved account is bound to the exact live ChatGPT PID and process birth time.
An absent, malformed, or previous-process marker makes a running APP display
`—`; the CLI quota remains independently resolved. Distribution, direct
`cxi switch`, and `cxi restart` bind the new Desktop process before recovery.
CLI-only changes update the marker's expected CLI account after committing
CLI auth and registry; a failed marker write never authorizes recovery IPC.
Distribution rechecks the registry and active CLI authentication after taking
the operation lock. During a Desktop relaunch the marker's CLI field follows
the account temporarily staged for Desktop; the final CLI commit updates it.
If a required journal or authentication write fails after shutdown, the
switcher attempts to relaunch the previous Desktop session and restore CLI auth.
An in-process logout or login that keeps the same ChatGPT PID cannot be
identified from process identity alone. Until Desktop exposes an authoritative
current-account read, treat that case as requiring a verified restart.

On this host the Command Line Tools Swift compiler and default macOS 27.0 SDK
have mismatched build versions. Until the tools are repaired, use
`SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk` and a writable
`CLANG_MODULE_CACHE_PATH` for Swift tests and installation; verify the exact
built app signature and running process after installation.

## Runtime Paths & Files
- `~/.codex/auth.json`: Active authentication tokens used by Codex CLI and `ChatGPT.app` (0600 permissions).
- `~/.codex/accounts.json`: Configured accounts database, multipliers, and cached quota metrics (0600 permissions).
- `~/.codex/usage-status.json`: Real-time quota snapshot consumed by the macOS Menu Bar app.
- `~/.codex/desktop-app-session.json`: Private exact-process APP account marker and expected CLI account; stale or unbound records are not authority.
- `~/.codex/ipc/` and `~/.codex/app-server-daemon/`: Official Codex Desktop IPC and app-server runtime state (preserved by the Monitor uninstaller).
- `~/.codex/thread-writer-locks/`: Active flock files held by Codex worker threads.
- `~/.codex/state_5.sqlite`: Thread metadata database used by the thread detection engine.

## Core CLI Commands
- `cxi status`: Check quota table across all accounts (`5H SPRINT`, `7D LIMIT`, `PLAN (MULT)`, `CREDITS`).
- `cxi switch <account>`: Switch active account (automatically recovers eligible quota-blocked or restart-captured turns; ambiguous active turns are not dispatched by discovery-only recovery).
- `cxi resume [thread-id]`: Resume an eligible quota-blocked or restart-captured thread through the Desktop owner's same-user IPC channel. An explicit ID also resumes a turn that ended with a non-quota error and no final agent message, and claims any stale deferred owner-wait for that task for the duration of the run (the journal keeps the original account binding and checkpoint, so a failure before dispatch or a crash leaves the deferred retry unchanged). When ChatGPT is closed and there is something to resume, it is started in the background first. Accessibility is used only for recovery visibility/banner verification, not to dispatch the turn.
- `cxi config`: Inspect and configure auto-switch modes (`--auto-switch-enabled`, `--auto-switch-business-only`, `--auto-switch-business-priority`, `--restart-app-on-switch`, `--preserve-window-bounds`).
- `cxi set-multiplier <account> <val>`: Set custom quota multiplier override (e.g. 20 for Pro 20x).
- `cxi reset-multiplier <account>`: Reset multiplier back to auto-detected default.
- `cxi wrap exec "<prompt>"`: Run unattended command with pre-flight quota check and auto-switch.

## Localization & Supported Languages
- Supported Languages (13 in the native app): English (`en`), Japanese (`ja`), Simplified Chinese (`zh-Hans`), Vietnamese (`vi`), Ukrainian (`uk`), German (`de`), French (`fr`), Spanish (`es`), Italian (`it`), Portuguese (`pt`), Polish (`pl`), Dutch (`nl`), Russian (`ru`). The offline `helps.html` guide supports the first 12; Russian is app-UI only.
- Native macOS Bundles: `resources/*.lproj/Localizable.strings` bundled inside `Codex Monitor.app/Contents/Resources/`.
- Offline Interactive Guide: `resources/helps.html` with language switcher and URL parameter routing (`?lang=ja`, `?lang=zh-Hans`, `?lang=vi`).

## Uninstall

Use `./scripts/uninstall.sh` from the checkout (the installer also bundles a
copy inside the Monitor app). It stops the Monitor and its
`launchd` workers, removes the Login Item, installed bundles, CLI/helper
shims, copied guide, app-owned logs/journals, app-specific support/cache/state
directories, preferences, and project skill links. It asks for confirmation; use `--yes` only for an explicitly approved
non-interactive run. Runtime status/recovery files are removed by the normal
uninstall; add `--purge-data` only to remove the Monitor-owned account registry
such as `~/.codex/accounts.json`.

During installation, log migration occurs only after the newly built app is
copied to same-filesystem staging and strictly signature-verified. The exact
Monitor app and daemon are then stopped; a Monitor PID is rechecked by executable
and start time immediately before signalling, while an in-flight restart worker
is allowed to finish and is verified absent rather than force-removed. The Rust helper redacts pre-existing active Monitor logs,
exact Monitor Brotli archives, and exact restart-worker logs with bounded
streaming and no-follow opens. It fails closed and preserves the source for
malformed, oversized, symlinked, or concurrently changed inputs; official
Codex Desktop logs and foreign archive entries are outside this scope. Bundle
activation uses a backup rename and restores the prior app if a later install
step fails. Runtime append/rotation takes a shared log-lifecycle lock while
historical migration takes it exclusively, so a retired inode cannot receive a
late write during redaction. Installation alone creates the coordination inode;
a stale writer therefore cannot recreate one after uninstall removes it, and a
waiter verifies the named inode again after flock acquisition. Daemon startup
validates the lock but never initializes it. Bundle
commit disarms rollback before best-effort backup cleanup.

The same operation is available in the running Menu Bar app: choose
`⛔ UNINSTALL CODEX MONITOR…` and type `UNINSTALL` in uppercase. The app then
starts its bundled uninstaller and exits. An unchecked option preserves
`accounts.json`; select it only when the Monitor's account registry should be
removed, or use the explicit `--purge-data` terminal option.

The uninstaller deliberately preserves official Codex Desktop and shared
state—`ChatGPT.app`, its bundled app-server, `~/.codex/auth.json`,
`state_5.sqlite`, `queue_1.sqlite`, sessions, thread locks, user `config.toml`,
and source checkouts. It cannot erase shell history, macOS unified logs,
notification history, LaunchServices caches, or TCC records; “no traces” means
no persistent Monitor installation artifacts, not forensic erasure of the OS.
