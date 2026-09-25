# 2026-09-25 — Cold task navigation and retry safety

- **Status:** Partial
- **Task/context:** Complete unattended recovery of cold ChatGPT Desktop tasks after account rotation.
- **Unexpected observation or failure:** Earlier URL launches returned success while Desktop owner discovery stayed at `no-client-found`. A new foreground URL opened one cold task, but a separate background URL also opened another, so the foreground flag alone does not explain every failure.
- **Evidence:** Both tasks were `notLoaded` before the test. The same-user Desktop IPC returned `no-client-found` before the first navigation and an owner after it. The second task gained an owner after a background URL. The installed app's deep-link handler also requires a successful thread read before navigation. Neither live test included an account change or a quota-interrupted turn.
- **Approaches tried:**
  - **Attempt:** Treat removing `open -g` as the complete root-cause fix.
    - **Outcome:** Partial.
    - **Why:** Foreground navigation mounted one task, but background navigation worked for another; the previous failures may depend on Desktop readiness, account state, or a failed pre-navigation read.
  - **Attempt:** Return URL-launch errors to recovery without preserving the original checkpoint.
    - **Outcome:** Did not work.
    - **Why:** A pre-dispatch launch error would have removed the task from the recovery journal. Adversarial review exposed the same loss on Desktop IPC startup, queue SQLite/revalidation, preparation, and rollout reads. Final pruning could also mistake a transient SQLite error for an ineligible task.
- **Root cause:** The installed Desktop sometimes does not turn an accepted external URL into a mounted task. The precise condition is still unconfirmed. Separate recovery-state bugs could discard an undispatched target after transient navigation, IPC startup, or queue read errors.
- **Resolution:** Activate ChatGPT once for an ownerless cold task, keep subsequent URL retries in the background, check the `open` exit status, retain pre-dispatch checkpoints on navigation, IPC startup, preparation, rollout, or queue failures, and make journal pruning fail closed on SQLite errors. The daemon reissues ownerless URLs at most once per minute under the same account binding. Dispatch remains gated by Desktop owner discovery and post-checkpoint turn proof.
- **Verification:** The foreground and background live links each mounted a cold task. Rust's 176 unit and 15 integration tests passed, including navigation arguments, cooldown, checkpoint retention on launch/IPC/queue errors, fail-closed journal pruning, and a deferred retry that cannot dispatch without an owner. The Swift test script passed with the compatible SDK. Full account-switch recovery and banner visibility over the resulting window remain unverified.
- **Prevention/follow-up:** Keep auto-switch disabled until the installed app completes a real cold quota-turn recovery and the banner is visible over the active ChatGPT window. Do not infer mounting from `open` exit code or weaken the owner gate.
- **Reusable learning:** Preserve a recovery checkpoint after any failure before IPC dispatch; only a verified Desktop owner can receive the recovery request.
- **References:** `codex-switcher/src/switcher/thread_identity.rs`, `codex-switcher/src/recovery/desktop_ipc.rs`, `codex-switcher/src/recovery/target_dispatch.rs`, `codex-switcher/src/recovery/deferred_recovery_service.rs`, `2026-09-25-chatgpt-cold-deep-link-read-guard.md`.
