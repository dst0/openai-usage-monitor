# 2026-09-27 — Unit tests ran the installed window helper and read the live process table

- **Status:** Resolved
- **Task/context:** Following [the CODEX_HOME and network hermeticity work](2026-09-26-tests-fell-back-to-live-codex-home-and-network.md), an adversarial review found `codex-switcher` unit tests that still touched the owner's live desktop.
- **Unexpected observation or failure:** Five tests passed while touching live state. The four `recovery::target_dispatch_tests::{explicit,queued}_{owner_wait,marker_write}_refuses_dispatch_after_shared_auth_changes` tests reached `~/.local/bin/codex-window-restore`. `switcher::tests::test_switch_to_account_rejects_relogin_needed` executed `/bin/ps`.
- **Evidence:**
  - Every unit test was run alone under `sandbox-exec` with `(allow default)` and `(with send-signal SIGKILL)` denials. The profile covered file access to `~/.codex`, `~/.local`, `~/Applications`, `/usr/local/bin`, and the ChatGPT, Codex Monitor, and Codex Notifier bundles; `process-exec` of `/bin/ps`, `/usr/bin/pgrep`, `/usr/bin/pkill`, `/usr/bin/open`, `/usr/bin/osascript`, `/bin/launchctl`, and `/usr/bin/lsappinfo`; and non-loopback network. On the base commit, exactly these five of 586 tests were killed. A probe showed that Rust's `posix_spawn` path makes the sandbox kill the test process itself, so each kill is attributable to its test. A shell fork-and-exec kills only the child.
  - The dispatch path went `mark_dispatch_attempt_with_writer`, then `recovery_account_binding(deferred)`, then `DesktopAccountBindingService::verified`. That lists ChatGPT PIDs with `/bin/ps`. With exactly one running, as on the development machine, it executes the installed helper's `inspect-process` against the live Desktop. The binding was computed for every deferred target, although an explicit request discards it.
  - `switch_to_account` ran both Desktop process probes before checking whether the target needed a relogin.
- **Approaches tried:**
  - **Attempt:** Move the relogin check ahead of the process probes.
    - **Outcome:** Rejected.
    - **Why:** It avoids `/bin/ps` for one test without giving any test a fake. The next test past preflight would read the live process table again.
  - **Attempt:** Add another closure parameter to `dispatch_if_needed` for the deferred binding.
    - **Outcome:** Rejected.
    - **Why:** It would give the function eight parameters, which Clippy's `too_many_arguments` rejects under `-D warnings`. The identity recheck and the binding are both "identity at checkpoint consumption", so they belong together.
  - **Attempt:** Add `DispatchIdentityChecks`, which carries the identity recheck and the deferred Desktop binding, and resolve the binding only for an unattended mode consuming an awaiting-owner checkpoint. Add `AccountSwitchPreflightService` with injected process probes behind `switch_to_account_with`.
    - **Outcome:** Worked.
    - **Why:** The four dispatch tests inject a binding that panics if consulted, which proves the explicit path no longer inspects the Desktop. New deferred-mode tests inject real bindings. The relogin test injects both Desktop states.
  - **Attempt:** Add a test-build tripwire, `test_live_system::forbid`, at the live seams: the three installed-helper resolvers, the single `/bin/ps` reader, and both ChatGPT task-link senders.
    - **Outcome:** Worked.
    - **Why:** A tripwire fires before the seam touches anything, whatever the machine's state. The sandbox only catches what the current machine happens to reach. With the tripwires added, exactly the five offending tests failed. Each tripwire has a guard test. If a tripwire were removed, its guard would at worst run a read-only probe: the helper guards would stat the helper paths and the process-table guard would run `/bin/ps`. The two task-link guards pass an invalid ID, so they would get a validation error instead of launching ChatGPT.
  - **Attempt:** Adversarial test review of the first version.
    - **Outcome:** Partial.
    - **Why:** Narrowing the marker's binding gate to `DeferredCaptured` survived the whole suite, and wiring the deferred binding to CLI auth was untested. `RestartWorkerDispatchService` still ran its own `/bin/ps`. The pinned task-link guard passed a valid ID, so without its tripwire it would have read the ChatGPT bundle's metadata. A mode-matrix test now covers the gate, a wiring test uses the tripwires as an oracle, the restart worker uses the shared reader, and the pinned sender validates its ID after the tripwire. The two mutants above now fail.
- **Root cause:** Live checks were evaluated eagerly, even where the result could not change the outcome. The seams that performed them were hard-wired rather than injected. Errors were swallowed (`SystemWindowRestoreBackend::new().ok()?`) or the test asserted only the final refusal, so passing tests never revealed the access.
- **Resolution:** Explicit resume no longer runs the helper or `ps` for a binding it ignores. `recovery_account_binding(bool)` became `deferred_account_binding()`, and its now-unused CLI branch was removed. The test-only `mark_dispatch_attempt_for_account` now drives the production marker. Helper resolution, every process-table read (including the restart worker's ancestry check), and both task-link senders refuse to run in unit tests.
- **Verification:** On the branch rebased onto main at 6792b6d, all 665 unit tests passed one at a time under the same kill-on-access sandbox, as did the full suite run in one process and the four integration binaries. `cargo test --locked` (three consecutive runs), `cargo fmt --check`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` passed. Mutation checks, run in a scratch copy under the sandbox, showed the tests catch a removed binding gate, a binding resolved eagerly in explicit mode, a gate narrowed to `DeferredCaptured`, and a deferred binding wired to CLI auth.
- **Prevention/follow-up:** AGENTS.md (Test Quality) now names the injected seams, the tripwire, and the sandbox audit. These seams have no tripwire of their own, and no test reaches them under the sandbox:
  - `send_macos_notification` (`osascript` and the notifier app);
  - `help_service` (`open`);
  - `shim::install_shim`, which creates entries in `~/.local/bin`;
  - `codex_binary_path`'s real CLI resolution, which stats the ChatGPT bundle;
  - `setup::relogin_service` and `interactive_setup`, which run the real `codex` CLI.

  The restart worker's `launchctl` calls and the `osascript` window-restore fallback run only after a tripwired read. A guard test for the unguarded seams could launch something or write something if its tripwire were removed, so any future tripwire there needs a guard design that stays inert in that case.
- **Reusable learning:** Resolve live checks lazily and behind injected seams. Make every live seam refuse to run in test builds, with a guard test that is itself safe. Audit with per-test kill-on-access sandboxing, because a denial alone is swallowed by error handling.
- **References:** `codex-switcher/src/test_live_system.rs`, `codex-switcher/src/recovery/dispatch_identity_checks.rs`, `codex-switcher/src/recovery/manifest_store.rs`, `codex-switcher/src/switcher/account_switch_preflight_service.rs`, `codex-switcher/src/switcher/codex_process_probe.rs`, `AGENTS.md`.
