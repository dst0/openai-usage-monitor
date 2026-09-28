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
For quota recovery, `state_5.sqlite` `updated_at` orders the 30 candidate
threads; the four-hour eligibility age comes from the latest quota-error
`task_complete` rollout event, not the SQLite metadata timestamp.
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
flag defaults to auto-switch off, and so does a registry without the setting
when the menu is built. Settings sync leaves a missing cache for the
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
Both reset journals are written by the shared `state_file` writer (exclusive
0600 staging, file flush, rename, directory flush) behind the injectable
`StateFileOperations` seam. If a manual attempt or automatic `pending` marker
cannot be saved or read back, nothing is sent; only a record that reads back
exactly as this write (its fresh key, or the new timestamp of a re-marked key
that was never sent) is withdrawn (manual: `resolved`; automatic: retryable
`journal_error`), and an unreadable or different record is left for
reconciliation. Manual reset refuses a request that cannot be built before it
records an attempt, as the automatic preflight does.
Manual and automatic reset paths use one recovery operation lock and check the
other path's unresolved journal before preparing a new request. Any unresolved
manual attempt blocks automatic reset across local IDs; pending/unknown
automatic attempts block a new manual or automatic key for the same account
route even when the cached weekly marker changes. The single automatic journal
cannot be replaced for another account while an attempt is unresolved. An unparsed HTTP status
never proves non-consumption. Restored quota does not clear an uncertain auto
journal or allow rotation; corrupt journal reads fail closed.
Automatic reset runs every final check (registry account and policy, weekly
quota and window marker, credits, threshold, live auth, a buildable request
route/token/key, Desktop) before it writes `pending`, then sends at once; a
refusal never leaves an unsent attempt unresolved. A refused retry of an
existing `pending`/`unknown` attempt, or one whose request could not be built
(`Unavailable`), is left unchanged with rotation suppressed because the first
request may have been applied; other retries are re-marked `pending` first.
Such a refused retry reports its own cause (`retry_refused:<cause>`,
`retry_unavailable:<reason>`) in status while the journal stays byte-identical.
The `Applied` follow-up (fresh usage read, Desktop recovery hand-off) runs
through `WeeklyResetEnvironment` and never depends on saving `applied`: no
later tick repeats it, because a visible `applied` returns early and a
`pending` that `applied` never replaced is held by the restored pool. A failed
`applied` write is retried after the hand-off and, if it fails again, reported
as an applied credit.
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
when Desktop IPC itself is healthy. The monitor activates ChatGPT once
for an ownerless task and requests background URL retries with
`open -g -a /Applications/ChatGPT.app`. If no owner mounts after at least
10 seconds of initial waiting, one native LaunchServices URL attempt uses the
exact `/Applications/ChatGPT.app` bundle while ordinary retries continue.
Foreground activation is acceptable; neither route's acceptance proves
mounting. A real IPC owner remains required before dispatch.
The pinned route uses deprecated `LSOpenFromURLSpec` and is bounded to an
ownerless retry; macOS compatibility must be rechecked after OS updates.
In that case automatic distribution reports
partial recovery rather than sending a turn without an owner. The daemon
retains only pre-dispatch tasks without a verified owner, including transient
URL-launch and Desktop IPC startup failures. It probes every 15 seconds and
reissues an ownerless task URL at most once per minute, alternating ordinary
and pinned native delivery only after actual attempts. Once
ChatGPT mounts the task, it retries using the original checkpoint and normal
turn verification. For a deferred retry after App/CLI distribution, the saved
Desktop account must match the target, the exact live ChatGPT PID and birth
identity, and the expected CLI account; an old or unbound session cannot
authorize dispatch. The initial recovery banner closes after its bounded
owner waits. A deferred navigation attempt for a still eligible target under
the exact saved Desktop account holds a new `Pending` banner through a bounded
owner-mount wait and hands that same panel to recovery after owner proof. If no
Desktop window is visible initially, panel creation is retried after navigation;
no IPC is sent unless a live panel appears. Timeout, identity change, or panel
failure retains the original checkpoint. While ownership is pending, the panel
says that tasks are being opened and checked; it does not claim a restart or
successful continuation before either is verified.
An older ownerless checkpoint stays eligible across another switch until a
new post-checkpoint turn has substantive, error-free agent work and no queued
follow-up. Only then does a later restart record a fresh offset and clear the
old account binding, including when the user resumed the task manually.
Metadata or a start without work never retires the retry.
Queued follow-ups require the same owner and turn-mode revalidation as an
unqueued turn. An already-unpaused queue still receives one owner-routed
`thread-follower-set-queued-follow-ups-state` wake after the durable dispatch
marker; owner discovery alone sends no work. Non-quota interrupted errors are
explicit-target only except for the operation-bound auth-refresh exception
described below; historical user Stop turns are not discovery-only recovery
candidates, including when a queue exists.
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
probes read newly appended bytes. The cursor and confirmation caches each hold
at most 128 entries and, when full, evict the least recently probed one, which
is normally a task that already left the journal. Replacing or pruning an ownerless retry
requires a second scan of the complete, unchanged snapshot. An incomplete or
malformed or oversized JSONL record anywhere in that interval cannot certify error-free work. Each prune pass
selects one ownerless task with a valid ID and a SQLite row, rotating per
`CODEX_HOME`, and does not tail-inspect the others; an invalid or unindexed
entry never takes that turn, and a single-target pass (the deferred worker's
re-prune before recovery) leaves the cursor alone so the next full pass
continues the rotation. A selected target with a stable terminal non-quota error is dropped
unless its saved, confirmed auth-rotation evidence is bound to the target
Desktop account; malformed or changed tails keep the retry. Older deferred intervals are scanned in chunks of at most 16 MiB
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
The deferred banner exists during an eligible navigation attempt and its
bounded owner wait, not between daemon probes. Unattended mounting after an
account switch remains unverified on the current Desktop build. If the daemon is not
running, inspect the affected task and use
`cxi resume <id>` only if the turn remains interrupted.

When a restart captures one selected window but has zero running recovery
targets, the banner shows a generic one-window pending message and no task rows.
The captured window and exact PID/birth identity still gate the panel. On
relaunch, target auth and the saved Desktop session are checked before the
panel is rebound or window geometry restored, then checked again immediately
before recovery IPC. The panel does not claim that a
task resumed; missing-window and optional-capture cases retain the prior
fail-closed or best-effort behavior for their configured capture mode.

An account switch records its recovery targets before shutdown and takes a
second checkpoint after the old ChatGPT process exits. The later offset keeps
shutdown-flushed events out of proof for the replacement turn. If that second
checkpoint cannot be saved, account switching stops before changing
`auth.json` and relaunches the previous Desktop account; a successful shutdown
alone is not permission to rotate credentials. `cxi restart` also relaunches
the previous Desktop state if its post-shutdown checkpoint fails.
Only a Monitor-owned A-to-B switch may mark the exact token-refresh failure
of an active pre-stop turn as automatically recoverable. Its private recovery
record carries the verified source and target account IDs, old turn ID,
first-checkpoint offset, rollout file identity, and the queue database identity,
revision, and pending count captured before that offset. The queue snapshot is
checked during preparation, after stop, and again before deferred dispatch.
A bounded streaming scan before
auth replacement must find the matching terminal error in that interval and
no Stop, new turn, or user input. The second offset remains the recovery proof
boundary. Deferred mounting and dispatch recheck the saved rollout identity,
length, and exact terminal interval, so replacement after confirmation cannot
inherit the exception. The exception still requires the exact relaunched target-account
Desktop session and a mounted IPC owner; `no-client-found` retains its
target-bound checkpoint without sending a turn. Historical auth errors without
this operation evidence remain explicit-only.
The recovery manifest reader rejects symlinks, changing files, and files over
1 MiB; serialized auth-rotation evidence must have bounded nonempty account
and turn IDs and a valid captured-rollout identity.
When eligible recovery targets or captured selected-window tasks exist,
distribution handshakes Desktop IPC after the first checkpoint and before
stopping ChatGPT. A failed handshake aborts the
switch, restores the prior recovery checkpoint, and clears the distribution
journal only after that restoration succeeds. The Desktop stays running.

Every CLI or distribution restart checks the exact ChatGPT PID, birth identity,
and WindowServer window inventory again immediately before SIGTERM. Direct
`cxi restart` and `cxi switch` require `--restore-window-tasks` for multiple
windows. A running-Desktop distribution captures every eligible window's task
in Rust, then requires exactly those window IDs at both checkpoint preflights
and immediately before shutdown. A verified zero-window snapshot remains an
exact empty list: a window opened afterwards blocks shutdown. A final helper
snapshot compares task, frame, focus, and keymap just before SIGTERM. A small
interval remains between that read and the signal, when Desktop could change
the selected task. An
unidentified, unreadable, changed, or duplicate window/task refuses the
restart before credentials change. This holds when
`preserve_window_bounds_on_restart=false`; that setting controls geometry only.

Desktop gives Monitor no window-to-task interface. Inspection of ChatGPT
26.924.22138 (read-only, from a copy of its bundle) shows that it persists
only one `electron-main-window-bounds` record, relaunches one primary window
without a task, sends a `codex://threads/<id>` link to its most recently
focused primary window (focus events update that choice), and opens a
focused primary window from File > New Window when its multiwindow feature is
on. Its IPC router has no window, route, or navigation method, and owner
discovery answers per host connection in the main process, never per window.
The explicit CLI flag and distribution's internal session build on exactly
those behaviors. Before shutdown
the helper reads each window's task with Copy deeplink and its Accessibility
frame, refusing the restart for a window without a task, a duplicate task, a
minimized, full-screen, or ambiguous window, a missing New Window item while
more than one window is open, a changed window list, or a keymap change.
After the relaunch and once Desktop IPC answers, it reuses the relaunched
window for one saved frame, waits up to 20 seconds for the New Window item
(the main process adds it only once the renderer reports the feature), opens
the others with it, sends each task link only while Accessibility shows the
target window focused (a focus change during navigation fails as
`NAVIGATION_TARGET_CHANGED`), checks the windows it already restored before a
second link, and counts a window only when it is on its frame and its own
Copy deeplink returns the planned task; a later link that moved an earlier
window is caught by a final pass. When recovery could have sent its own task
link (a target no restored window showed, or an incomplete restore), even when
recovery failed, a recheck moves back only a window now showing a recovery
task instead of its own, with the same link guard and a final pass after
every attempted link, including a last attempt whose target never verifies; it
creates, moves, and closes nothing, cannot tell recovery's link from the
user's own navigation to a recovery task, and reactivates the app that was
frontmost before it. The recheck reports any unmatched frame or unreadable
Copy deeplink as unverified, without navigating that window. A rehearsal treats an unreadable liveness
check as an unclosed extra window and reports failure; after a new-window
inventory error it attempts to close the known focused new window. Its final
WindowServer/Accessibility inventory must exactly match the pre-rehearsal IDs;
an extra window or unreadable inventory reports `REHEARSAL_WINDOW_LEFT_OPEN`.
The plan reaches the helper on stdin
and the snapshot returns on stdout; each task link is handed to macOS `open`,
as recovery already does. Every window-task helper run has a deadline and is
killed with its process group when it passes; the helper also exits once its
parent is gone. For both distribution restore passes, task links wait for
Desktop IPC; target authentication, session marker, PID, and birth are checked
immediately before the helper and again after it returns. Independent Desktop
changes can still interleave with a link. Capture and restore foreground
Desktop windows and briefly use the clipboard. Restore failures, including a
relaunch that never reached the restore, are reported with the restart result
and never block recovery.
When a post-stop distribution failure relaunches the previous account, its
captured task session remains active until that relaunch returns a verified
previous-account Desktop session. The same IPC, auth, session-marker, PID,
and birth checks guard its restore; only then is the session finished. If the
previous relaunch or restore cannot be verified, the error reports incomplete
window restoration, while the distribution journal follows the auth rollback
result.
Only a command whose raw `--trigger` is `user`
accepts the flag; the
daemon, the Monitor app, distribution, and auto-switch never pass that CLI flag;
distribution owns its task session inside the Rust lifecycle
(`tests/enforce_task_probe_isolation.rs` pins every caller and the forwarded
value). None of this has run against a live Desktop yet;
`cxi window rehearse-task-restore --allow-focus-and-clipboard` exercises
focus-targeted navigation and New Window in temporary extra windows without a
restart, then gives them their opening frames back and closes only them. It
cannot show cold-task loading, New Window timing, or window reuse after a
relaunch.

An explicit diagnostic, `cxi window probe-tasks --allow-focus-and-clipboard`,
tests whether the installed Desktop exposes a one-to-one mapping through Copy
deeplink. It is evidence gathering only and has not been run against a live
multiwindow Desktop: it foregrounds each window, lets ChatGPT replace the
clipboard with each task link, puts the previous clipboard (or a newer copy made
between its copies) back when no other app wrote after its last copy, and neither persists task IDs nor authorizes
restart. The probe, the rehearsal, and the restore share these checks. Before
any visible change they require the account's `~/.codex` as the Codex home
(resolved from the user database, not `$HOME`), hold the switch/recovery
operation lock for the whole run, and refuse a keymap entry that names
`copyDeeplink` or binds the L key, more than one ChatGPT process, a build
other than 26.924.22138 (11645) or a bundle changed since launch, a macOS
App Shortcut on Cmd+Opt+L, a keyboard layout on which that key does not type
`l` with Command held, missing Accessibility or event-posting access,
pasteboard access set to deny, and minimized windows. ChatGPT 26.924.22138
binds its hidden `copyDeeplink` command to Cmd+Opt+L by default (no other
default uses it) and reads overrides from `$CODEX_HOME/keybindings.json`, an
array of `{command, key}` entries: a command named by an entry uses exactly
those keys, and every other command keeps its defaults. So Cmd+Opt+L still
means Copy deeplink when no entry names `copyDeeplink` or binds L, which is
the accepted keymap (the owner's dictation override passes). An edit still
present when the command ends voids the result. A ChatGPT started with its
own `CODEX_HOME` is not detected. The Desktop copies the link of
`BrowserWindow.getFocusedWindow()` and otherwise falls back to its primary
window, so the helper sends the shortcut only to the verified ChatGPT PID,
reads keyboard focus live from Accessibility before and after each copy, and
requires one clipboard write without a concealed or transient marker that
stays unchanged while it reads the link. On macOS 15.4 and later the system
may ask before that read; a prompt that takes focus makes the command fail
closed. It also rejects ambiguous AX/WindowServer frame matches,
process/window drift, and duplicate or invalid task links. The probe and the
rehearsal never output task IDs, and the CLI prints only counts or a fixed
failure code, adding a note when a failure may have followed a focus change.
Each copied link is on the system clipboard, where other apps, clipboard
history, and Universal Clipboard can see it; one competing write of a valid
task link still cannot be attributed. Private or larger than 32 MiB clipboard
contents are neither read nor restored.
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
IPC and retains the original checkpoint if no window appears. The shutdown
window guard always checks Accessibility and Screen Recording inside the
`codex-window-restore` process launched by `cxi`, including when
`preserve_window_bounds_on_restart=false`.
`WINDOW_ACCESSIBILITY_DENIED` and `WINDOW_SCREEN_RECORDING_DENIED` are fixed,
non-sensitive pre-signal failures. Neither check prompts for a grant. The
installer accepts an explicit `CODEX_MONITOR_SIGNING_IDENTITY_SHA1` and signs
both the CLI and window helper with fixed identifiers; without it, both are ad
hoc signed and a TCC grant may not survive rebuilding. Verify the installed
launchd path and the actual TCC-attributed client rather than an
interactive Terminal run. Disabling bounds preservation skips geometry
restoration but cannot authorize a shutdown with unreadable window inventory.
The legacy LaunchAgent has no `AssociatedBundleIdentifiers` key: Apple requires
the agent executable and associated app to share a Team Identifier, while the
installer currently signs the Monitor app ad hoc. Do not add that key as a
privacy-grant workaround without matching signatures and live verification.

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
Automatic switching stays disabled until quota-interrupted cold tasks complete
end-to-end recovery and exact selected-task restoration across multiple windows
is verified in the installed app. Historical logs show URL-to-owner-to-IPC
recovery; current live checks show that accepted URL delivery may leave the
task ownerless at an immediate check. The installed ChatGPT 26.924.20706
deep-link handler shows its primary window before ordinary task navigation,
which the owner accepts. No supported background mount IPC method was evident.
These checks dispatched no recovery turn or account switch. URL acceptance is
not owner or recovery proof.

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

On this host the Command Line Tools Swift compiler (`6.4.0.34.1`) and the
default macOS 27.0 SDK's Swift interfaces (`6.4.0.31.4`) come from different
builds. That difference alone does not block builds: with a writable module
cache, the default SDK builds every installer target and passes
`./scripts/test_swift.sh` (checked 2026-09-28). The Swift interfaces of both
installed SDKs were built by a different compiler, so when Swift cannot rebuild
one of their modules, for example because the module cache is not writable, it
may report `this SDK is not supported by the compiler`. Read the error printed
before that line. Where a sandbox denies the default cache, set
`CLANG_MODULE_CACHE_PATH` to a writable directory.
`SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk` still works
but is not required.

A module cache works only at the exact path that built it. Clang records
imported module files by absolute path, so reusing a warm cache through another
spelling of its directory (`/tmp/x` for `/private/tmp/x`, or any other
symlink), or after copying or moving it, fails. The error depends on the
toolchain and the source. For a second spelling, Swift 6.4 reports
`module '_DarwinFoundation1' is defined in both ...`; `swift-frontend` then
crashed with signal 11 on the installer's `codex-ui-resume.swift` and on
`tests/ScreenContrastTests.swift`, but not on a small `import Darwin` probe.
Swift 5.10 on GitHub's `macos-14` runner reports
`PCH was compiled with module cache path ...` and
`missing required module 'SwiftShims'` instead, and Swift 6.4 reports the same
recorded-path mismatch for a copied or moved cache. Fresh caches and same-path
reuse work.

`scripts/install.sh` and `scripts/test_swift.sh` therefore never compile into
`CLANG_MODULE_CACHE_PATH` itself. They create it if it is missing, resolve it
to its physical path (`cd -P` and `pwd -P`, which on this host also fold
`/PRIVATE/TMP`, `/System/Volumes/Data/...`, `//`, and `/./` into one
spelling), and use its subdirectory
`codex-monitor-swift-<cksum of the physical path>`. Every spelling reaches the
same subdirectory, nothing else writes it, a cache another tool warmed through
an alias is left alone, and a copied or moved cache gets a new subdirectory;
delete old `codex-monitor-swift-*` directories to reclaim space. A value that
already is that subdirectory for its parent, such as the value the helper
exports to a nested script, is used as is. A relative path resolves once
against the caller's directory. The scripts stop if the path is
not a directory, contains a newline, or starts with an unexpanded `~`, or if
the subdirectory is a symlink, is not a directory, is not owned by the user
or is writable by group or others (another account could plant modules that
get compiled into the app), or is not writable and searchable (`[ -w ]`,
`[ -x ]`, which also apply ACLs and sandbox rules). A
read-only cache compiles only what it already holds for the same flags, and
these scripts compile with several. Other `swiftc`, `swift`, or
`clang -fmodules` runs that share a cache among themselves must use one
spelling, and must not reuse a copied or moved cache.
Verify the exact built app signature and running process after installation.

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
- `cxi switch <account>`: Switch active account (automatically recovers eligible quota-blocked or restart-captured turns; ambiguous active turns are not dispatched by discovery-only recovery). `--restore-window-tasks` captures each ChatGPT window's task before the restart and reopens it afterwards (focuses windows, uses the clipboard; unverified live).
- `cxi restart [--restore-window-tasks]`: Restart Desktop and verify recovery without changing accounts; the flag works as for `cxi switch`.
- `cxi window probe-tasks --allow-focus-and-clipboard` / `cxi window rehearse-task-restore --allow-focus-and-clipboard`: explicit diagnostics for the window-to-task mapping and for the restore steps; neither restarts Desktop.
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
such as `~/.codex/accounts.json`. Interrupted Monitor staging files are removed
by two lists. The shell list matches the journal, Desktop-session,
`manual-reset-state.<pid>.<16 hex>.tmp.json`, and the Monitor's
`auth.json.<pid>.<16 hex>.tmp` credential copy by exact name, current owner,
`0600` mode, and regular non-symlink file. The fd-anchored `monitor-logs`
helper removes the other Monitor staging names by prefix and suffix as regular
non-symlink files, without an owner or mode check. Any new or renamed staging
writer needs a matching pattern and shell test in the same change.

Leftovers of a killed install are a third list. `install.sh` takes its install
lock before creating any temporary path and releases it only as the last step
of its EXIT cleanup, after removing them. The lock file is
`codex_monitor_install_<uid>.lock` in `getconf DARWIN_USER_TEMP_DIR`, whatever
`TMPDIR` says, so an installer and an uninstaller started from a shell,
launchd, or the Menu Bar app meet at one file; the installer refuses to run
without that directory. Both scripts take the lock through an identical
`flock_fd_now`: `/usr/bin/lockf`'s descriptor form where it exists (macOS 15
and later), otherwise `/usr/bin/perl`'s `flock` (macOS 13 and 14 ship no
`lockf`), otherwise the installer fails closed and the uninstaller treats the
lock as unverified. An installer waits by polling, because `lockf` waits on a
descriptor by spinning a CPU. The uninstaller removes a free lock file while
it holds that lock, so an installer that was waiting for it would then hold
a file without a name; after every acquisition the installer compares the
open file (`stat 0<&9`; `stat /dev/fd/9` reports devfs's device) with the
path and locks the file now at the path when they differ. It writes its PID
through the locked descriptor, never by path, and then hands the lock to a
perl keeper that is forked with INT, TERM, HUP, and QUIT ignored and exits
within about 0.05 s of the installer's exit. Once the keeper reports that it
runs, the installer closes its own descriptor, so no command it runs, such
as a compiler cache server started under cargo, can inherit the lock and
keep it after the installer ends; if the keeper never reports, the installer
keeps the descriptor and says so. An interrupted installer keeps the lock
through its EXIT cleanup (it traps QUIT too, which bash would otherwise let
end it without that cleanup). Both scripts run perl with `-T`, which ignores
the caller's `PERL5OPT` and `PERL5LIB`. Before its remote clone, CLI
staging, bundle staging, and bundle swap, the installer stops if the path
no longer names its lock file or a fresh probe no longer finds the lock
held: uninstallers from before this lock removed a lock file once more after
releasing it, and a keeper can be killed. The uninstaller's probe
opens each lock file read-only, never creating one, likewise starts over
when the path no longer names the file it locked, and treats any lock-tool
status other than taken or held as unknown.

A confirmed uninstall stops with exit status 75 before changing anything
while an installer holds that lock, or a legacy `${TMPDIR:-/tmp}` or `/tmp`
one of an older installer; its dry run says so and marks each held lock
file. It does not stop an installer that starts during the uninstall, whose
leftovers the check below still keeps. The Menu Bar app discards the
uninstaller's output, so a refusal started from the menu is silent. The
uninstaller matches `~/.local/bin/.codex-mon.install.XXXXXX` (regular file,
mode `0600` or `0755`) and its codesign `.cstemp` copy
(regular file, `0755`), `.codex-monitor-install.XXXXXX` and
`.codex-monitor-backup.XXXXXX` in `/Applications` and `~/Applications` (mode
`0700` directory that is empty or holds only a real `Codex Monitor.app`
directory), and the remote-install clone `codex-mon-install-XXXXXX.` plus ten
characters (eight on macOS 13) in `getconf DARWIN_USER_TEMP_DIR` (mode `0700`
directory). Each `X` is one of mktemp's `[0-9A-Za-z]`, the owner must be the
current user, and symlinks are never followed. A backup can hold the only
copy of the previous app while an install runs, so these are removed only
when every lock file is absent or its `flock` can be taken at once, checked
again right before removal. A held lock, or one that is symlinked, not a
regular file, another user's, unreadable, or in an unknown per-user
temporary directory, keeps them and makes the uninstall report warnings. The
Monitor lock files (`daemon.lock`, `codex.lock`, `monitor.lock`,
`desktop-recovery.lock`, and the install locks) are removed the same way,
only while their lock is free. After the installer exits, a backup it kept
because rollback failed is Monitor-owned debris and is removed with the app.
The uninstaller also removes its own interrupted
`<file>.codex-monitor-uninstall.XXXXXX` copies of `~/.zshrc`,
`~/.bash_profile`, and `~/.codex/config.toml` (current owner, regular file).
`tests/log_permissions_and_uninstall.sh` fails when an installer or
uninstaller `mktemp` template changes, and `tests/install_lock.sh` runs the
installer's own lock and EXIT cleanup; the required Rust CI job runs both on
every pull request.

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
Compiler build caches are outside its scope too: the default Clang module cache,
and the `codex-monitor-swift-*` subdirectory that the install and test scripts
create in a `CLANG_MODULE_CACHE_PATH` you set. Delete those by hand if wanted.
