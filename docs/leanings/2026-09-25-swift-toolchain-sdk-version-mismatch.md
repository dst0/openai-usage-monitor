# 2026-09-25 — Swift compiler and default SDK version mismatch

- **Status:** Corrected
- **Correction:** 2026-09-28. This record blamed the failure on the difference between the compiler and default SDK builds. A controlled rerun on the unchanged toolchain disproved that. With a writable module cache, the default macOS 27.0 SDK built all four installer targets in 3 of 3 runs and passed `./scripts/test_swift.sh`. An unwritable cache produced the same "SDK is not supported" message with the 26.5 SDK as well (3 of 3 runs each). The version difference is real, but the denied module cache caused the failure. Corrected the explanations of both attempts, the root cause, the resolution, and the reusable learning. Evidence: [2026-09-28 — Swift's "SDK is not supported" error came from an unwritable module cache](2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md).
- **Task/context:** Build and test the native banner and macOS installer on this host.
- **Unexpected observation or failure:** A simple Swift import failed before compiling project code.
- **Evidence:** The selected Command Line Tools compiler reported Swift 6.4 build `6.4.0.34.1`, while the default macOS 27.0 SDK's Swift interface reported `6.4.0.31.4`. The default module cache also lay outside the current writable sandbox. Setting a writable module cache and using the installed macOS 26.5 SDK allowed the banner geometry test and full Swift test script to pass.
- **Approaches tried:**
  - **Attempt:** Compile against the default SDK and module cache.
    - **Outcome:** Did not work.
    - **Why:** Module cache access was denied, so the SDK's Swift modules could not be rebuilt from their interfaces. Swift reported that as an unsupported SDK.
  - **Attempt:** Set `SDKROOT` to the installed macOS 26.5 SDK and `CLANG_MODULE_CACHE_PATH` to a writable temporary directory.
    - **Outcome:** Worked for the native build and tests.
    - **Why:** The cache is writable. Changing the SDK was not needed (see the correction).
- **Root cause:** The sandbox denied writes to the default compiler module cache, so Swift could not rebuild the SDK's modules. The host's compiler and default SDK do come from different Command Line Tools builds, but that difference alone does not block compilation.
- **Resolution:** Use a writable temporary module cache for this installation. The explicit 26.5 SDK also works but is not required. Do not change system toolchain symlinks as part of the project fix.
- **Verification:** The banner geometry test and `./scripts/test_swift.sh` passed with the explicit environment. `./scripts/install.sh` completed with the same environment; strict signature verification passed for the installed app and helpers, the app binary hash matched the staged build, and Monitor and its daemon were running.
- **Prevention/follow-up:** Check compiler, SDK Swift interface, and cache writability before native builds; document the exact temporary override and verify the installed signed artifact afterward.
- **Reusable learning:** A native compilation failure can precede project code when the environment blocks the compiler, for example with an unwritable module cache. Isolate the toolchain cause before editing source.
- **References:** `CODEX.md`, `scripts/test_swift.sh`, `scripts/install.sh`.
