# 2026-09-27 — Swift module cache alias interrupted Monitor installation

- **Status:** Resolved
- **Task/context:** Rebuild and reinstall the verified Monitor after merging the external APP binding fix into the recovery branch.
- **Unexpected observation or failure:** The installer stopped while compiling the first Swift helper, after the Rust CLI had been updated but before replacing the Monitor app bundle.
- **Evidence:** Swift reported `_DarwinFoundation1` defined in both `/private/tmp/...pcm` and `/tmp/...pcm`. On this host `/tmp` resolves to `/private/tmp`. The original Monitor process and signed app bundle remained in place after the failed attempt.
- **Approaches tried:**
  - **Attempt:** Reuse a writable `CLANG_MODULE_CACHE_PATH` under `/tmp`.
    - **Outcome:** Did not work.
    - **Why:** Swift encountered two path spellings for the same cached module and aborted compilation.
  - **Attempt:** Give this install a fresh cache under the canonical `/private/tmp` path.
    - **Outcome:** Worked.
    - **Why:** The helper, notifier, and Monitor compiled without duplicate PCM identities.
- **Root cause:** A symlink-alias cache path was reused across Swift invocations on a host with a compiler/SDK mismatch; Swift's module loader treated the canonical and alias names as duplicate definitions.
- **Resolution:** Re-ran installation with the matching SDK and a fresh canonical cache path. Documented the path rule in the installation guidance.
- **Verification:** Installation completed; the installed Monitor binary matched the signed build, strict code-signature checks passed for app and CLI, a new Monitor and daemon started, and the same ChatGPT Desktop PID remained active.
- **Prevention/follow-up:** Use a fresh canonical module-cache directory for each installation when overriding Swift's cache path. Check the installed app, CLI, and process identity after any failed or retried install.
- **Reusable learning:** On macOS, avoid `/tmp` symlink aliases in a shared Swift module-cache path; use a unique canonical `/private/tmp` directory.
- **References:** `scripts/install.sh`, `CODEX.md`, `README.md`.
