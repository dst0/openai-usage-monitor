# 2026-09-25 — Swift compiler and default SDK version mismatch

- **Status:** Partial
- **Task/context:** Build and test the native banner and macOS installer on this host.
- **Unexpected observation or failure:** A simple Swift import failed before compiling project code.
- **Evidence:** The selected Command Line Tools compiler reported Swift 6.4 build `6.4.0.34.1`, while the default macOS 27.0 SDK's Swift interface reported `6.4.0.31.4`. The default module cache also lay outside the current writable sandbox. Setting a writable module cache and using the installed macOS 26.5 SDK allowed the banner geometry test and full Swift test script to pass.
- **Approaches tried:**
  - **Attempt:** Compile against the default SDK and module cache.
    - **Outcome:** Did not work.
    - **Why:** Module cache access was denied and compiler/SDK build versions were incompatible.
  - **Attempt:** Set `SDKROOT` to the installed macOS 26.5 SDK and `CLANG_MODULE_CACHE_PATH` to a writable temporary directory.
    - **Outcome:** Worked for the native build and tests.
    - **Why:** The older Swift interface is readable by the current compiler and the cache is writable.
- **Root cause:** The host's selected compiler and default SDK come from mismatched Command Line Tools builds; the sandbox additionally restricts the default compiler cache.
- **Resolution:** Use the explicit SDK and temporary module cache for this installation until Command Line Tools are repaired. Do not change system toolchain symlinks as part of the project fix.
- **Verification:** The banner geometry test and `./scripts/test_swift.sh` passed with the explicit environment. Installer verification is pending.
- **Prevention/follow-up:** Check compiler, SDK Swift interface, and cache writability before native builds; document the exact temporary override and verify the installed signed artifact afterward.
- **Reusable learning:** A native compilation failure can precede project code when the selected compiler and SDK builds differ; isolate the toolchain cause before editing source.
- **References:** `CODEX.md`, `scripts/test_swift.sh`, `scripts/install.sh`.
