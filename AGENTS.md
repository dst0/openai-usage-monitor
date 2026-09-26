# Codex Switcher & Monitor — Agent Guidance & Code Culture

## Core Architecture & Language Boundaries

1. **Rust-First Portable Core (`codex-switcher/`)**:
   - Quota monitoring, account switching, worker orchestration, daemon management, thread resumption, and CLI commands (`cxi`, `codex-mon`) are 100% native Rust.
   - Rust is the primary language for high-performance, minimal-memory background execution, resilient IPC transport, and offline-first data structures.
   - `~/.codex/ipc/ipc.sock`: Same-user Desktop IPC router used by recovery to address the window that owns a task.
   - `~/.codex/app-server-daemon/`: Desktop-owned app-server coordination state; it is shared with the official app and is preserved by the Monitor uninstaller.
   - `~/.codex/auth.json`: Active credentials used by Codex CLI and ChatGPT.app.
   - `~/.codex/accounts.json`: Multi-account credentials and quota cache (POSIX 0600 permissions).
   - `~/.codex/usage-status.json`: Real-time cache consumed by the Swift status bar app.
   - `~/.codex/thread-writer-locks/`: Active flock files held by running Codex app-server worker threads.
   - `~/.codex/state_5.sqlite`: SQLite database tracking thread metadata (`updated_at`, `rollout_path`, `archived`, `thread_source`).
   - `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`: Event logs for all thread turns and tool executions.

2. **Swift Native macOS Integration (`Sources/`)**:
   - Swift and AppKit are strictly dedicated to native macOS presentation, system status bar items, menus, user interactions, accessibility, and vibrancy-free composite image rendering (`Codex Monitor.app`).
   - Swift must not contain separate switching or account orchestration logic; it delegates account switching, monitoring, and recovery directly to the Rust core (`codex-mon` / `cxi`).
   - `/Applications/ChatGPT.app`: The standard OpenAI Codex Desktop application. Its bundled `codex app-server` is the only Desktop thread writer; the Monitor does not install or launch a second one.
   - Installation may place the Monitor bundle in `/Applications` or `~/Applications` depending on write access; a system install also leaves a `~/Applications/Codex Monitor.app` symlink.

3. **Continuous Documentation & Codebase Alignment**:
   - Keep all documentation (`README.md`, `CODEX.md`, `AGENTS.md`, specs) continuously synchronized with any changes in code, architecture, or design decisions.
   - Discrepancies between documentation and implementation must never be ignored.
   - If documentation is outdated, update it immediately in the same change.
   - If a documented feature is missing or incomplete, implement it to bring implementation into compliance with documentation.

### Installation & Removal Contract

- `scripts/install.sh` installs the Monitor/Switcher components only; the
  official `/Applications/ChatGPT.app` and its bundled app-server are neither
  patched nor replaced. It may use `/Applications` or `~/Applications` for the
  Monitor bundle and retains remote-install skill sources in
  `~/.local/share/codex-monitor/`.
- After same-filesystem staging and strict signature verification, the installer
  must stop the exact Monitor app and daemon, revalidate a Monitor PID's exact
  executable and start time immediately before signalling it, allow any in-flight restart worker
  to finish without force-removing it, verify all writers absent, then redact pre-existing
  exact Monitor active logs, timestamped Brotli Q6 archives, and restart-worker
  logs with bounded fd-anchored no-follow streaming. Invalid, oversized,
  symlinked, or concurrently changed sources fail closed and remain in place;
  foreign files and official Codex Desktop logs are never rewritten. Directory
  enumeration errors must fail closed rather than being accepted as EOF. App-bundle
  activation must use rollback-capable rename semantics that preserve the prior
  bundle on staging, activation, or later installation failure; commit must
  disarm rollback before best-effort backup cleanup. Runtime log
  append/rotation must hold a shared lifecycle lock and historical migration
  must hold it exclusively, preventing late writes to a retired inode. Only
  installation may create the lock, so a writer cannot recreate it after
  uninstall removes the locked inode; daemon validation must never create it,
  and a waiter must recheck the named inode after flock acquisition.
- Runtime and historical log redaction must fail closed on incomplete quoted
  credentials and multiword unquoted sensitive values. Validate complete generic
  members after quoted secrets, reject malformed unquoted punctuation, cache
  trusted boundaries, and bound total suffix scanning. Scan wrapped tokens and every
  identifier in list-valued diagnostics rather than stopping at the first match.
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

## Code Culture & Rust Coding Standards

- **Symbol Locality**: Every Rust struct (and its implementation) and every trait must reside in its own dedicated file. Multi-struct grab-bag files are strictly prohibited.
- **Maximum File Length**: No `.rs` source file may exceed **300 lines** (excluding `mod.rs`, `lib.rs`, test-only files, and generated files). This is enforced by automated test suites (`tests/enforce_file_limits.rs`, `codex-switcher/tests/enforce_file_limits.rs`, and `benchmarks/floem-codex-simulator/tests/enforce_file_limits.rs`). When a file grows beyond the limit, split it by extracting logical groups (e.g., helper functions, structs, enums, serde types, sub-impl blocks) into sibling files in the same module.
- **Naming & Extraction Policy**: Split modules must be logically grouped and meaningfully named. Prohibit generic suffixes like `part1.rs`, `part2.rs`, `_read.rs`, `_write.rs`, or `_impl.rs`. Decompose complex logic into specialized Named Services (e.g., `QuotaService`, `RecoveryService`, `SwitchStrategyService`, `AutoResetService`) or move significant structs to their own dedicated files rather than arbitrary file chunks.
- **Thin Coordinator Pattern**: Coordinators, CLI handlers, daemon dispatchers, and entry points must be thin orchestrators. They should handle transport/CLI/dispatch concerns (CLI argument parsing, IPC framing, signal trapping, response formatting) but delegate all complex domain logic to named services.
- **Named Service Extraction**: Move complex logic from handlers into dedicated service structs named after the operation they perform. These services should maintain their own state (e.g., storage handles, configuration) via dependency injection rather than massive procedural helper functions.
- **Splitting Workflow**: When splitting a file to comply with the 300-line limit:
  1. Extract code into new sibling files.
  2. Register new modules in parent `mod.rs` / module root.
  3. Run `cargo check` to verify compilation.
  4. Run relevant tests to verify behavior.
  5. Commit the working state before moving to the next file. Never batch-split multiple files without verifying compilation between each.
- **Test Locality & Extraction**: Inline `#[cfg(test)] mod tests` blocks that push a file over the limit should be extracted to a sibling `<filename>.test.rs` file (e.g. `recovery.rs` → `recovery.test.rs`), referenced via `#[cfg(test)] #[path = "<filename>.test.rs"] mod tests;` in the source file. The enforcement test excludes `.test.rs` files from the line count.
- **Grouping**: Use modules and packages to group related symbols hierarchically.
- **Avoid Index Pattern**: Avoid the "index pattern" (re-exporting everything from submodules via wildcard `pub use *` in `mod.rs` or `lib.rs`). Require explicit hierarchical navigation.
- **Logical Constant Placement**: Constants must be located in the files where they logically belong (e.g., limits for a struct in the same file as the struct). Avoid generic `constants.rs` or `types.rs` dump files.
- **Test Locality**: Tests must be split into separate files in the same or similar meaningful way as the code they verify. Avoid large monolith `tests.rs` or inline `#[cfg(test)]` blocks for complex logic.

---

<!-- destinationworks-universal-agent-baseline:v1 -->
## Universal Delivery Baseline (v1)

These rules are the portable minimum for Destination Works repositories. Repository-specific instructions may strengthen them or name concrete commands, but must not silently weaken them.

### Evidence, scope, and decisions

- Read the repository instructions and relevant canonical docs before changing files. Check available cross-session memory when prior decisions or recurring failures may affect the work.
- While actively working, reread a repository-root `user_updates.md` at least once per minute when it exists. Treat new entries as user instructions, handle them before continuing, remove only entries that were fully handled, and never delete the file itself.
- Establish the live baseline before diagnosing or claiming completion. Prefer direct evidence from current code, tests, CI, deployed artifacts, or authenticated system state over comments, stale reports, or agent summaries.
- Preserve unrelated and user-owned changes. Use an isolated branch/worktree for broad work, stage intentionally, and never reset, clean, delete, or rewrite unrelated state to simplify a task.
- For non-trivial changes, compare 2-3 viable approaches and record the decisive tradeoffs. Proof-test material assumptions with a focused reproduction or authoritative source before committing to the design.
- Test scripted replacements and bulk mechanical edits on a disposable copy of one representative file before applying them broadly; inspect the result for collateral changes.
- Keep implementation, user/setup documentation, architecture/runtime contracts, and operator guidance synchronized in the same change.
- Store closed, well-compressible logs and temporary evidence with Brotli quality 6 when practical. Never compress an actively appended log as one stream: rotate or close it into chunks first, then compress each completed chunk. Use a format better suited to append, random access, or unsupported tooling when required, and record the reason for that exception.

### Durable learning capture

- Maintain `docs/leanings/` as the repository-wide learning collection. Add exactly one Markdown file per learning in the same work that reveals a material resolved bug/regression, failed or misleading experiment, unexpected behavior, setup/environment trap, non-obvious constraint, important workaround, or rejected approach with reusable rationale; routine successful work needs no entry.
- Record the task/context, observable symptom, sanitized decisive evidence, approaches tried and why each worked or failed, root cause or honest uncertainty, resolution, verification, prevention/follow-up, reusable rule, and safe references. Use `Resolved`, `Partial`, or `Open` status truthfully.
- Follow `docs/leanings/README.md` for filenames and structure. Keep learning files append-only by default. Correct prior understanding with a new linked file rather than rewriting history.
- Exception: when authoritative evidence proves an existing statement was fabricated, hallucinated, or factually false, correct or remove the false content so it cannot mislead future work. Mark the entry `Corrected` and add a dated note stating what was wrong, the authoritative evidence, and what changed; never use this exception for disputed interpretation, ordinary staleness, or changed external conditions.
- Promote the shortest prevention rule into the appropriate canonical instructions, setup guide, architecture contract, or operator runbook in the same change. Do not leave durable knowledge only in chat, commit history, a PR, or the learning collection.
- Never record secrets, credentials, private keys, customer data, sensitive payloads, device codes, or unsanitized production evidence.

### Validation and test quality

- Discover and use the repository's canonical commands (`cargo test` in `codex-switcher`, `./scripts/test_swift.sh` for AppKit test suites); do not invent shared command names where the project does not define them.
- Use a validation ladder: fast targeted feedback while iterating, the repository pre-commit gate before commit, and the full pre-push/release-relevant gate before push. If a named gate does not exist, run the closest repository-native equivalent and document the exact evidence.
- A hook is developer feedback, not the authoritative merge gate. CI must rerun required checks from a clean checkout.
- Never weaken, skip, or replace a failing check merely to make it green. Read the failure, fix the cause, rerun the narrowest relevant test, then rerun the containing gate.
- After rebasing a PR onto a newly merged main branch, rerun Clippy on the combined tree; compiler-version-sensitive lints can appear in the new base even when the PR's own patch is unchanged.
- Validate generated artifacts against their source and canonical generator. Do not hand-edit generated output or accept drift.
- Tests must cover meaningful behavior, negative/error paths, and important boundaries. Coverage is a regression signal, not a reason to add vacuous line-fillers or bypass comments.
- For non-trivial or high-risk changes, obtain an independent adversarial review of assumptions, tests, failure handling, and rollback before publication.
- Process-timeout tests must prove that descendants and inherited pipes are gone, not merely that the direct child received a signal. When an external Unix `kill` command receives a negative process-group operand, terminate option parsing with `--` and cover the Linux path.
- For user-visible UI changes, exercise the changed path in the real installed application after automated tests pass; record the nearest honest evidence if UI automation is unavailable.

### Git, pull requests, and CI enforcement

- Start from current remote truth, keep commits scoped and reviewable, and verify the exact staged diff before committing. Do not mix unrelated work into one PR.
- Always include a `## Bug Fixes` section in the PR description detailing any bugs uncovered and resolved during the task, with references to their regression tests.
- A local pass, push, or successful agent report is not proof that remote CI passed. Confirm the remote PR head SHA and every required check on that exact revision.
- Self-merge only when branch/ruleset protection actually enforces the required checks and they all pass. If protection is unavailable, checks cannot start, or the head changed after validation, leave the PR open for owner approval.
- CI workflows must use least-privilege permissions, pinned third-party actions, explicit timeouts/concurrency, and repository-owned validation commands.
- PR descriptions must explain why the change was needed, what changed, approaches rejected, exact validation, bugs found/fixed with regression evidence, learning-log entries, risk, and rollback.

### Security and supply chain

- Never store or expose credentials, tokens, private keys, customer data, sensitive payloads, device codes, or unsanitized production evidence in source, logs, fixtures, PRs, or learning records.
- Treat whitespace, commas, and closing punctuation after a quoted sensitive value as structural only after validating the complete suffix; a forged closer or key name alone leaves the suffix sensitive.
- Enforce POSIX `0600` permissions on all credential and token files (`auth.json`, `accounts.json`).
- Ensure atomic file operations (`fs2` flock) when accessing credentials.
- Zero credential leakage: automated security scans reject any potential API key patterns or secrets.
- Treat dependency lifecycle scripts, lockfile changes, generated code, binary downloads, workflow actions, and base images as reviewed supply-chain inputs. Pin immutable versions/digests where supported and fail on unreviewed drift.
- Security-sensitive configuration and deployment paths must fail closed when required identity, authorization, signing, backup, or runtime prerequisites are missing.
- Treat configuration loading, validation, and TLS/HTTP client construction as separate fallible boundaries. Propagate and sanitize errors from each boundary instead of assuming validation makes construction infallible.
- Privileged installers must pin `PATH` and executable paths, reject symlinks, copy into root-owned same-filesystem staging, verify the staged identifier and exact signer, atomically rename, and roll back after any later failure.

### Release and deployment integrity

- When the repository publishes a deployable artifact, build it once, identify it by immutable digest, and test the exact bytes that will be promoted on every published platform.
- Rebuild, code-sign, and reinstall the macOS app bundle (`Codex Monitor.app`) cleanly; verify code signatures strictly (`codesign --verify --strict`).
- Separate immutable provenance tags from mutable environment pointers. Publish and verify evidence first, move the smallest mutable production pointer last, verify the live promoted state, and define an exact rollback to the previously recorded digest.
- Rehearse backup/restore and rollback through safe isolated commands that produce inspectable evidence; documentation-string checks alone are not operational proof.

<!-- /destinationworks-universal-agent-baseline:v1 -->

---

## Test Quality & Adversarial Review

- Tests must never be added solely as mechanical line-fillers to pass coverage gates. Tests must meaningfully verify domain logic, invariant preservation, realistic crash recovery, positive cases, negative cases, and edge cases.
- Bug fixes must start with a reproducible failing regression test before writing the fix.
- Panic-path cleanup guards must never panic while another panic is unwinding. Release internal locks before invoking callbacks, recover poisoned coordination locks during cleanup, and cover the poisoned-lock path deterministically.
- For non-trivial features, bug fixes, or test additions, automatically spawn an adversarial test-critic subagent to review the tests. The critic must evaluate whether the suite verifies real behavior vs artificial line coverage, identifies missing edge cases, and flags fragile/vacuous tests before work is completed.
- Never use coverage bypass comments to bypass coverage gates. All ordinary code in the repository must be reachable and exercised by tests; dead or unreachable code must be deleted rather than kept or suppressed.
- If you create or modify a test file, run it and iterate on test or implementation until it passes.

---

## Mandatory Learning Log

- Maintain the repository-wide append-only learning collection in `docs/leanings/`.
- Create exactly one Markdown file per learning in the same change whenever work reveals a resolved bug or regression, failed or misleading experiment, unexpected behavior, setup or environment trap, non-obvious constraint, important workaround, or rejected approach with reusable rationale.
- Routine successful work does not need an entry unless it produces a reusable insight.
- Follow the filename convention and exact entry structure documented in `docs/leanings/README.md`. Include the task/context, observation or failure, evidence, approaches tried and their outcomes, root cause, resolution, verification, prevention or follow-up, and the reusable learning.
- Mark uncertainty honestly. If root cause or resolution is incomplete, record the entry as `Partial` or `Open` and state what evidence is still missing.
- Keep learning files append-only by default: do not delete or rewrite older files merely to make the history cleaner. Put later discoveries in a new file that links the earlier learning.
- Link relevant issues, commits, logs, or regression tests when safe and useful.
- Never place credentials, tokens, private keys, customer data, sensitive payloads, or unsanitized production evidence in learning files.

---

## Thread Detection & Resumption Engine

**Condition of use:** All ChatGPT accounts configured in this Monitor belong to
the same human owner of this device. Cross-account continuation of that owner's
local Codex tasks is the intended recovery behavior. Keep this condition
distinct from the Desktop limitation: an accepted deep link may still fail to
mount a cold task, so owner discovery remains mandatory before dispatch.

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
| Desktop IPC startup | up to `90 s` | Waits for the standard Codex Desktop same-user IPC socket, validating owner, mode, peer UID, and socket identity. |
| Desktop owner discovery | up to `90 s` | Resolves the Desktop window that owns a task; a deep link is used only when no owner exists. |
| Deferred owner probe | minimum interval `15 s` while daemon is running | Retries only a pre-dispatch target after ChatGPT has mounted an owner under the same account; daemon ticks or another running recovery may lengthen the interval, and the four-hour eligibility window still applies. |
| IPC recovery dispatch | `90 s` | Bounds the wait for Desktop to accept one owner-routed recovery request and start the expected turn. |
| Recovery execution | `600 s` | Allows a started task to produce substantive new agent work before failing closed. |
| Recovery evidence soak | `10 s` | Requires substantive work to remain error-free before declaring the recovered turn verified. |
| Desktop stability | `3 s` | Requires the same singleton Desktop PID throughout; verifies a visible window only when one was captured before restart. |
| Banner minimum visibility | `5 s` | Keeps the recovery banner visible when a visible window and recovery target were found. |

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
- **Owner discovery**: `cxi` connects to `~/.codex/ipc/ipc.sock`, asks `thread-owner-discovery` for the task owner, and activates ChatGPT once with a `codex://threads/<tid>` deep link only when Desktop reports no owner for a cold task. Repeated links stay in the background; already-owned tasks are never cycled through the UI. An accepted link is never a substitute for owner proof.
- **Multiple windows**: In an already running Desktop, resolve and dispatch each task through its own owner, even when several windows belong to the same account. Singleton process validation applies to the ChatGPT main PID, not to the number of windows. The current Desktop interface has no verified per-window selected-task mapping or targeted window navigation, so restarting multiple windows cannot yet preserve their task selection. Keep automatic switching disabled until this path is proven.
- **Cold task fallback**: If Desktop never mounts that deep link, the recovery journal retains only targets whose owner was unavailable before any request was sent, including URL-launch and Desktop IPC startup failures. The daemon probes for a real owner every 15 seconds and reissues an ownerless task URL in the background at most once per minute. For deferred retries after App/CLI distribution, it uses the saved Desktop account only when the exact live ChatGPT PID and birth identity and expected CLI account match that session; legacy unbound sessions fail closed. It resumes from the original checkpoint once a real owner mounts it, rechecks queue and rollout state after owner discovery, and durably removes the retry intent before any IPC request. Unattended mounting is not guaranteed by the current Desktop build; no unknown-outcome IPC send is retried. A bounded initial banner may close while the target still awaits an owner; deferred dispatch requires a fresh visible banner.
- **Restart checkpoint replacement**: Save targets before shutdown and again after the old Desktop exits. The post-shutdown offset excludes shutdown-flushed events from replacement-turn proof. A retained ownerless retry for the same task is superseded only when a new `task_started` is observed after its old checkpoint; metadata alone preserves the original checkpoint and account binding. A post-checkpoint turn start followed by substantive, error-free agent work retires a stale retry that the user already resumed only when no queued follow-up remains. Existing queued follow-ups keep their retry eligible; a start alone is insufficient. If the second checkpoint fails during an account switch, leave the old auth in place and relaunch the previous Desktop account.
- **Recovery journal integrity**: Reject duplicate task IDs in recovery manifests on read and write; otherwise one task could carry conflicting offsets or account bindings. Scan the full post-checkpoint interval through a captured rollout-length snapshot with fixed-size read buffers and bounded line storage; do not strand a long-running turn behind a fixed total-byte cutoff. If `cxi restart` cannot save its post-shutdown checkpoint, relaunch the previous Desktop state before returning the error.
- **Deferred scan cost**: Ordinary `load_pending()` considers restart targets only; ownerless entries belong to the deferred worker and must not cause repeated SQLite retries or rollout scans during detection. Cache only a bounded append cursor and lifecycle evidence for deferred post-checkpoint scans; recheck the live Desktop account and owner on every probe. Invalidate the cursor on rollout path/inode change or truncation. Scan at most one ownerless rollout per locked prune pass, rotate that selection, and inspect at most 16 MiB of an older interval in each probe. Report no lifecycle evidence until the captured snapshot end is reached; never impose a fixed total-byte cutoff. Confirm cached evidence with a complete fresh, unchanged-length scan before replacing a saved account binding or pruning an undispatched retry.
- **Deferred banner policy**: Recovery in an already running Desktop captures banner placement through read-only WindowServer geometry with exact PID and birth checks. An initial `WINDOW_NOT_FOUND` may defer panel creation until after owner discovery; before IPC, a visible panel with a live helper is required. Recheck queue revision/count and rollout after panel startup, recheck helper liveness and exact Desktop identity after any SQLite wait, then recheck queue/rollout after that final panel check before marking dispatch durably. Window access/geometry failure, panel timeout, changed identity, missing helper, payload/lease failure, malformed helper output, and unknown failures retain the original checkpoint. Replay earlier task statuses into a late panel; replay write failure blocks dispatch. This path never restores window bounds or requires an Accessibility window read.
- **Cold-task navigation limit**: A successful macOS `open` exit for a deep link does not prove the task mounted. Live checks found cold tasks that stayed `no-client-found` after accepted links; native task navigation mounted them, after which owner-routed recovery produced verified work. The unattended account-switch path remains unverified. Report partial recovery and never dispatch to an unverified owner; inspect the turn before a later explicit `cxi resume <id>`.
- **Interrupted turn dispatch**: For a task without a pending queue, `cxi` sends exactly one `thread-follower-start-turn` request to the owner with the protocol-valid text `continue` and `turnTrigger = app_update_resume`.
- **Queued follow-ups**: Existing queued payloads are preserved. `thread-follower-set-queued-follow-ups-state` is used to remove only the exact restart pause reason; user-paused queues are rejected.
- **Verification**: A dispatch/IPC acknowledgement is not success. Proof requires the exact returned turn ID, a post-checkpoint `task_started`, substantive agent work, a 10-second error-free soak, and Desktop stability verification. Visibility is verified when a window was captured before restart.
- **Window preservation policy**: With `preserve_window_bounds_on_restart=true`, a denied Accessibility read, invalid geometry, or process mismatch blocks auth changes and restart. With the setting explicitly false, distribution still verifies the exact singleton Desktop PID and birth identity, then uses WindowServer geometry to place a banner for visible recovery targets. It skips position/size restore and the Accessibility-based visible-window check while retaining IPC recovery and PID stability checks. A cosmetic capture failure may allow the credential rotation itself; owner-routed task dispatch still requires a live panel after mounting and retains the checkpoint if it cannot show one. Process identity, helper protocol, and unknown capture failures block rotation. Never interpret `WINDOW_ACCESS_FAILED` as `WINDOW_NOT_FOUND`.
- **Banner coordinates**: WindowServer/Accessibility rectangles are top-down; AppKit panels are bottom-up. Convert using the owning display's CoreGraphics and NSScreen bounds. Choose the display with greatest visible overlap, including partially offscreen windows.
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
