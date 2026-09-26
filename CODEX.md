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

ChatGPT Desktop starts its bundled `codex app-server`. The Monitor is only a
same-user IPC client: it connects to `~/.codex/ipc/ipc.sock`, discovers the
Desktop window that owns a task, and routes `thread-follower-start-turn` or
queued-follow-up requests to that owner. The Desktop-owned app-server remains
the only thread writer; the Monitor never runs `codex exec resume`.
Desktop and CLI read the same `auth.json`, so their targets must select the same
account even when Desktop is closed and may launch later. Explicit split
requests are rejected; CLI-only credential writes are rejected while Desktop
runs. Background quota polling uses the saved access token without OAuth
refresh or writes to active authentication. Identity checks reject a nonblank
access, refresh, or ID token saved under a
different account, including when two providers share one email. They allow
unique same-account rotation and ignore blank optional token placeholders.
The watchdog does not shorten the configured polling interval or scan recent
quota-blocked tasks when both automatic switching and weekly auto-reset are
disabled. Weekly auto-reset retains the rapid blocked-task probe when it is
enabled independently of switching.
The running Desktop owns refresh-token rotation; after its exact main process and
bundled writer exit, the switcher saves the latest Desktop token to the account
registry before replacing `auth.json`. Failed process inspection or ambiguous
identity blocks the switch. Before shutdown, the live authentication must
uniquely match both planned account identities; stale session markers and
registry entries cannot authorize the stop. If registry persistence fails after
a shutdown-time token rotation, a guarded relaunch of the same identified
account retains the failed handoff journal. Keep automatic switching disabled
until cold-task mounting and restoration of selected tasks across multiple
windows are proven.
Desktop distribution replaces all target token fields, clears the previous
account's API key, and uses exact shared-auth compare-and-replace for both the
forward write and rollback. A concurrent credential change blocks either write;
uncertain outcomes retain the journal for inspection.
A stale journal after shutdown begins, in an auth commit, relaunch, or unknown
phase blocks subsequent distribution until live auth, active registry account,
recovery checkpoint, and Desktop session are reconciled. Distribution loads
only a same-user regular `0600` journal of at most 16 KiB through a
no-follow, stable-inode read. It writes through random exclusive `0600`
staging, atomic rename, and directory sync; unsafe existing paths fail closed.
The Desktop session marker follows the same `0600`, 16 KiB, no-follow,
stable-inode read and random exclusive staging rules. Invalid markers block
account distribution rather than being treated as absent.
If a marker save reports an error after rename, restore the previous marker
before clearing the distribution journal; retain the journal if restoration
cannot be verified.
Offline distribution commits and rolls back only the active-account
field in a freshly locked registry; it never restores an old whole-file snapshot.
Rollback must prove the previous account's exact credential binding before
restoring auth and again before clearing the distribution journal.
Direct `cxi switch` resolves a requested account to its stable ID before the
fresh registry sync, then requires exactly one matching ID in the refreshed
registry before selecting credentials; a removed or duplicated target blocks the switch.
It rechecks target credentials and eligibility after the post-shutdown sync,
then durably records a private, bounded `direct-switch-journal.json` before
the first auth write. The journal stores account IDs and SHA-256 fingerprints,
never token values. Under the recovery operation lock, the next direct switch
or distribution reconciles it: an exact prior auth/registry pair clears the
intent; an exact target auth with the prior active ID may finalize only that
registry field; changed or ambiguous state blocks further switching. A live
shared-auth writer blocks reconciliation until it exits. The journal alone
does not prove that Desktop mounted or resumed a task. A post-stop failure
before the auth write relaunches the prior Desktop only after exact prior auth
and registry readback; an uncertain prior state leaves it stopped.
After auth replacement, it commits only the selected active ID into a freshly
locked registry and rejects a changed prior active ID or a removed, duplicated,
reauthenticated, or newly ineligible target. Direct switching compare-writes
the existing shared auth document, replaces the token object without copying
prior-account token extensions, clears `OPENAI_API_KEY`, and verifies the whole
auth document again after registry commit. A first switch with no auth file
creates it without replacing a concurrent file and conditionally removes that
exact new file on rollback when its content and inode still match. Failed auth
rollback or changed post-commit auth leaves Desktop
stopped for inspection.
Journal inspection, stale cleanup, candidate planning, and commit share one
recovery operation lock. Offline auth replacement checks again for a Desktop
writer immediately before writing; a failed marker save restores the previous
auth and registry when possible, otherwise retains the journal. A stop error
after signalling Desktop preserves its journal and recovery checkpoint.
Before-signal stop rejection and checkpoint preparation errors clear the
distribution journal only after the prior recovery checkpoint is restored;
failed rollback leaves the journal intact. Offline auth replacement verifies
process state and auth readback after the write and after registry/marker
persistence; Desktop registry commit verifies a final
auth readback before reporting success.
Shutdown token handoff and post-relaunch commit update the latest account
registry under one lock; re-login, daemon sync, first-run import, and duplicate
auto-heal use the same fresh locked merge. Full-document auth readback includes unknown fields.
Configuration and registration also mutate only their fields under a fresh
registry lock; quota HTTP remains outside it. A stale whole-file save can
re-enable auto-switch or restore an old refresh token.
Interactive setup propagates registry and active-auth errors and uses a private
random login directory. The derived status cache copies current registry
switch settings while holding the same lock; an absent or malformed Swift cache
flag defaults to auto-switch off. Settings sync leaves a missing cache for the
daemon to populate with a complete quota snapshot. Status staging uses the same
unpredictable, exclusive no-follow temporary-file pattern as credential staging.
Manual reset-credit consumption commits only the credit cache by stable account
ID with token, route, and previous-credit checks. A conflict after the remote
service reports `Applied` is reported as consumed with an uncertain local cache;
the request is never retried with a new idempotency key.
Manual `reset-account` writes `manual-reset-state.json` durably before the
service call. A pending, unknown, or applied-but-uncommitted attempt blocks all
later manual reset requests; known non-consumption and applied plus committed
cache resolve it. Operator reconciliation requires official same-account quota
evidence before removing the exact 0600 journal path, as described in README.
Manual and automatic reset paths use one recovery operation lock and check the
other path's unresolved journal before preparing a new request. Any unresolved
manual attempt blocks automatic reset across local IDs; pending/unknown
automatic attempts block a new manual or automatic key for the same account
route even when the cached weekly marker changes. The single automatic journal
cannot be replaced for another account while an attempt is unresolved. An unparsed HTTP status
never proves non-consumption. Restored quota does not clear an uncertain auto
journal or allow rotation; corrupt journal reads fail closed.
Credential and registry staging uses unpredictable `create_new`, `O_NOFOLLOW`,
mode-0600 temporary files instead of reopening a predictable filename.
Active-auth reads open with `O_NOFOLLOW`, require a private regular file, and
recheck the named inode after reading.
Re-login requires a usable decoded email claim from the official CLI login
matching the selected account and a non-default workspace ID; conflicting
ID/access token emails fail closed. JWT signatures are not verified locally.
Active-auth compare-and-write uses an advisory
Monitor lock plus final file and process checks, but Desktop does not honor
that lock, so a concurrent launch cannot be claimed race-free.
Process-table probes accept ordinary system PID 0/1 rows and apply target PID
checks only to exact ChatGPT executable matches; malformed rows still fail closed.
The credential-writer guard recognizes both the older bundled
`Contents/Resources/codex` executable and newer `Contents/Resources/codex-cli/`
layout.
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
An older ownerless checkpoint stays eligible across another switch until a
new post-checkpoint turn has substantive, error-free agent work and no queued
follow-up. Only then does a later restart record a fresh offset and clear the
old account binding, including when the user resumed the task manually.
Metadata or a start without work never retires the retry.
Queued follow-ups require the same owner and turn-mode revalidation as an
unqueued turn. An already-unpaused queue still receives one owner-routed
`thread-follower-set-queued-follow-ups-state` wake after the durable dispatch
marker; owner discovery alone sends no work. Non-quota interrupted errors are
explicit-target only, and historical user Stop turns are not discovery-only
recovery candidates, including when a queue exists.
Tail classification reads only through a captured file length, checks whether
the seek begins on a record boundary, discards a partial first record as bytes
before strict UTF-8 decoding, and requires a final newline. A malformed newer
record yields unknown state rather than inferring activity from an older event.
The evidence scan covers the complete post-checkpoint interval up to a captured
rollout length, using fixed-size read buffers and a bounded line buffer. Large
legitimate turns do not lose their retry solely because they exceed a byte
cutoff. Recovery manifests with duplicate task IDs fail validation on both
read and write, so two entries cannot claim conflicting account bindings.
The deferred worker keeps a bounded append cursor as a hint, so ordinary
probes read newly appended bytes. Replacing or pruning an ownerless retry
requires a second scan of the complete, unchanged snapshot. An incomplete or
malformed or oversized JSONL record anywhere in that interval cannot certify error-free work. Each prune pass
selects one ownerless task, rotating per `CODEX_HOME`, and does not tail-inspect
the others. A selected target with a stable terminal non-quota error is dropped
because unattended recovery cannot dispatch it; malformed or changed tails
keep the retry. Older deferred intervals are scanned in chunks of at most 16 MiB
per probe and yield no lifecycle result until the snapshot end is reached. Foreground
recovery scans at most 16 MiB of rollout payload per pass across its targets,
plus small boundary samples, and waits for a complete newline-terminated
snapshot before sending IPC. Growth or changed metadata after an earlier
pre-dispatch scan retains the checkpoint for a fresh attempt because an append
could hide a rewrite in already-read bytes. File replacement, truncation, or
changed identity resets or rejects the cursor;
account and Desktop-owner checks remain fresh on every probe. Ordinary thread
detection excludes ownerless entries from `load_pending()` because the deferred
worker owns those retries.
All foreground recovery modes pin the verified active auth account and exact
ChatGPT PID and birth identity when recovery begins. They recheck both after an
owner wait, before and after consuming the checkpoint, immediately before
owner-routed IPC. A pre-send mismatch restores the original checkpoint with
readback and blocks both queued and turn-start dispatch. Finalization keeps an
explicitly claimed deferred target's original offset and owner binding. A
previously unbound target keeps its offset but becomes bound to the account
that started recovery, so the changed account cannot retry it unattended. A
hidden captured-restart banner can bind the
relaunched process only when the exact live Desktop session marker matches;
other modes reject a process change during the wait.
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
When eligible recovery targets exist, distribution handshakes Desktop IPC after
the first checkpoint and before stopping ChatGPT. A failed handshake aborts the
switch, restores the prior recovery checkpoint, and clears the distribution
journal only after that restoration succeeds. The Desktop stays running.

Every CLI or distribution restart checks the exact ChatGPT PID, birth identity,
and WindowServer window inventory again immediately before SIGTERM. More than
one user window, or an unidentified window that could be user-owned, stops the
restart before credentials change. The current Desktop interfaces do not give
Monitor a stable mapping from each window to its selected task, nor a way to
reopen a specific task in a specific new window. This guard applies even when
`preserve_window_bounds_on_restart=false`; that setting controls geometry only.
The helper rejects malformed or non-finite WindowServer bounds and rechecks
the Desktop process birth immediately before each Accessibility geometry write.
Window title and geometry heuristics alone cannot prove that a window is
non-user-owned; ambiguous inventory blocks the restart. Automatic switching
stays disabled until exact selected-task restoration across windows is proven.
The helper excludes an unnamed offscreen renderer only when its frame differs
from every Accessibility standard-window frame. An unexpected or malformed
offscreen title, or an overlapping standard frame, blocks shutdown. This
allows the observed single-window Desktop while still leaving Accessibility
window-roster completeness unproven.

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
`—`. Rust writes a CLI quota identity only when live authentication matches
the selected account, and the Swift reader checks the exact `auth.json` file
identity before displaying it. An unknown or replaced CLI auth file displays
`—` instead of using the first cached account or top-level quota. Distribution, direct
`cxi switch`, and `cxi restart` bind the new Desktop process before recovery.
APP and CLI targets must name the same account because both share `auth.json`;
a failed marker write never authorizes recovery IPC.
Distribution rechecks the registry and active CLI authentication after taking
the operation lock. During a Desktop relaunch the marker binds both fields to
the target account before recovery waits.
Immediately before owner-routed IPC, live `auth.json`, the exact relaunched
Desktop PID and birth identity, and the saved session marker must still match
the planned account; a mismatch blocks dispatch.
If a required journal or authentication write fails after shutdown, the
switcher attempts a verified relaunch of the previous Desktop account.
A failed second checkpoint relaunches and binds the previous APP account without
dispatching recovery requests; the relaunch saves verified same-account token
rotation before clearing its journal. An offline switch first saves the old
account's verified token rotation and compare-writes from that auth snapshot.
A ChatGPT relaunch outside Monitor can renew only a prior process-bound APP
marker for the same uniquely registered account. The private shared auth file
must have mtime, birthtime, and ctime before the new process and contain
complete tokens matching the active registry entry. The daemon checks process,
marker, and auth identities again
around its atomic marker write. Inferred bindings retain the auth file identity;
Swift and distribution reject them after that file changes. Otherwise APP stays
`—` until a verified restart. This does not prove an account switch inside one
unchanged Desktop process.
A shared-auth commit that rolls back returns
an error, not a successful distribution status.
An in-process logout or login that keeps the same ChatGPT PID cannot be
identified from process identity alone. Until Desktop exposes an authoritative
current-account read, treat that case as requiring a verified restart.

On this host the Command Line Tools Swift compiler and default macOS 27.0 SDK
have mismatched build versions. Until the tools are repaired, use
`SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk` and a writable
`CLANG_MODULE_CACHE_PATH` for Swift tests and installation. Use a fresh,
canonical `/private/tmp/...` cache path rather than the `/tmp` symlink: the
Swift compiler can otherwise load the same PCM under both names and fail with
a duplicate-module error. Verify the exact built app signature and running
process after installation.

## Runtime Paths & Files
- `~/.codex/auth.json`: Active authentication tokens used by Codex CLI and `ChatGPT.app` (0600 permissions).
- `~/.codex/accounts.json`: Configured accounts database, multipliers, and cached quota metrics (0600 permissions).
- `~/.codex/usage-status.json`: Real-time quota snapshot with a CLI auth-file identity; the macOS Menu Bar app rejects CLI quota attribution after `auth.json` changes.
- `~/.codex/desktop-app-session.json`: Private exact-process APP account marker and expected CLI account; stale or unbound records are not authority.
- `~/.codex/ipc/` and `~/.codex/app-server-daemon/`: Official Codex Desktop IPC and app-server runtime state (preserved by the Monitor uninstaller).
- `~/.codex/thread-writer-locks/`: Active flock files held by Codex worker threads.
- `~/.codex/state_5.sqlite`: Thread metadata database used by the thread detection engine.

## Core CLI Commands
- `cxi status`: Check quota table across all accounts (`5H SPRINT`, `7D LIMIT`, `PLAN (MULT)`, `CREDITS`).
- `cxi switch <account>`: Switch active account (automatically recovers eligible quota-blocked or restart-captured turns; ambiguous active turns are not dispatched by discovery-only recovery).
- `cxi resume [thread-id]`: Resume an eligible quota-blocked or restart-captured thread through the Desktop owner's same-user IPC channel. Accessibility is used only for recovery visibility/banner verification, not to dispatch the turn.
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
