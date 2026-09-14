# Recovery verification — 2026-09-14

## Final status

- **PASS — recovery-only:** Desktop IPC accepted one protocol-valid `continue` input for task `01a0979a-70f4-7cb3-8588-528e11ba5294`. Turn `01a09c26-8ea9-78c3-a50b-e827112b83cb` produced substantive work, had no abort/error, and passed the 90-second soak. Video: `/Users/dst/Desktop/codex-recovery-only-ipc-pass-2026-09-14.mov` (180.008 s, H.264, 2560x1440, Display 3).
- **PASS — full self-restart:** Codex main PID changed exactly once, `78485 -> 92337`, and was launched automatically. Four user tasks received four distinct IPC-confirmed turns, each produced real work and passed a 90-second error-free soak. The same singleton PID then passed a separate 90-second stability window and exact-PID visible-window check. Worker terminal result: `WORKER_RESULT passed`. Video: `/Users/dst/Desktop/codex-full-self-restart-ipc-pass-2026-09-14.mov` (480.008 s, H.264, 2560x1440, Display 3).
- **PASS — Monitor Quit cancellation:** a restart delayed by 15 seconds was scheduled, then Codex Monitor was explicitly quit. The private `0600` cancellation marker was written, the recurring daemon unloaded, the worker returned `WORKER_CANCELLED phase=pre_shutdown`, and Codex remained PID `92337`. Relaunching Monitor cleared the marker and restored the daemon. Video: `/Users/dst/Desktop/codex-monitor-quit-cancellation-pass-2026-09-14-v2.mov` (90.010 s, H.264, 2560x1440, Display 3).
- **PASS — post-run state:** the recovery manifest is empty, active authentication did not change, no new Codex crash report appeared, the Monitor daemon is running, and no second restart cycle occurred.
- **PASS — static gates:** Rust tests `72/72`, Clippy with warnings denied, Rust formatting, both Swift suites, Swift helper typecheck, release install/signature checks, `git diff --check`, and the repository `.mjs` prohibition all pass.

## Failures found and fixed during verification

- The first recovery-only test correctly failed with Desktop IPC `EmptyInput`: the request used `input: []`. The installed Desktop schema requires `[{"type":"text","text":"continue","text_elements":[]}]`. Failure video: `/Users/dst/Desktop/codex-recovery-only-empty-input-fail-2026-09-14.mov`.
- The first full restart after that fix launched Codex and resumed three real tasks, but correctly ended failed because stale subagent IDs entered the manifest through fail-open SQLite/primary-task paths and the final AX-dependent visibility check false-negatived. Both ingress paths now fail closed; stale manifest entries are revalidated, and visibility is proven by an on-screen layer-0 CGWindow belonging to the exact PID. Failure video: `/Users/dst/Desktop/codex-full-restart-stale-manifest-fail-2026-09-14.mov`.

## Invariants

Recovery uses the existing Codex Desktop owner's IPC connection, never a second app-server or headless `codex resume`. A dispatch/IPC acknowledgement is not success. Proof requires the exact returned turn ID, a post-checkpoint start, substantive work, a 90-second error-free soak, and then stable/visible Desktop verification. Recovery manifests contain UUIDs and offsets only, are atomically replaced with mode `0600`, and never contain prompts, transcripts, account data, or credentials. Active logs remain plaintext for live inspection; completed logs are archived with Brotli Q6.

The native Accessibility helper is not a recovery transport and does not press
Play, Resume, Retry, or Steer controls. It is limited to the best-effort
recovery banner and visibility checks; authoritative Desktop visibility proof is
bound to the exact main-process PID and its on-screen layer-0 window.
