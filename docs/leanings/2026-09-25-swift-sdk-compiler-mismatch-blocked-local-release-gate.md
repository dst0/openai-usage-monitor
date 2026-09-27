# 2026-09-25 — Swift SDK/compiler mismatch blocked the local release gate

- **Status:** Corrected
- **Correction:** 2026-09-28. This record called the compiler/SDK version difference "independently decisive". A controlled rerun on the unchanged toolchain disproved that. With a writable module cache, the default SDK built all four installer targets in 3 of 3 runs and passed `./scripts/test_swift.sh`. An unwritable cache produced the same "SDK is not supported" message with the 26.5 SDK as well (3 of 3 runs each). The denied module cache caused the failure. Corrected the evidence, attempt, root cause, resolution, prevention, and reusable learning; the title keeps the original diagnosis for continuity. Evidence: [2026-09-28 — Swift's "SDK is not supported" error came from an unwritable module cache](2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md).
- **Task/context:** Validating the Codex Monitor cold task recovery change before reinstalling the macOS application.
- **Unexpected observation or failure:** `./scripts/test_swift.sh` failed while building its first suite, before any changed Swift source was compiled.
- **Evidence:** `xcode-select -p` selected `/Library/Developer/CommandLineTools`. The active compiler reported Swift `6.4.0.34.1`; the selected macOS SDK Swift interface reported `6.4.0.31.4`. Swift reported that it could not build the SDK's `Swift` module and called the SDK unsupported. The sandbox also denied writing the default Clang module cache, and that denial is what made the rebuild fail.
- **Approaches tried:**
  - **Attempt:** Run the repository's canonical Swift test script.
    - **Outcome:** Did not work.
    - **Why:** The sandbox denied the default module cache, so the SDK's Swift modules could not be rebuilt.
- **Root cause:** The sandbox denied writes to the default module cache. The Command Line Tools compiler and SDK builds do differ, but that alone does not block compilation. The change does not touch Swift sources.
- **Resolution:** The Rust gate passed, and local Swift validation was left pending. A writable `CLANG_MODULE_CACHE_PATH` is enough to run it.
- **Verification:** `xcrun swiftc --version`, `xcode-select -p`, and the first Swift test failure establish the mismatch; the Swift suite has not passed in this checkout.
- **Prevention/follow-up:** Rerun `./scripts/test_swift.sh` before installing, signing, or promoting the application. The README prerequisites state this gate.
- **Reusable learning:** Treat a Swift toolchain failure as an environment release blocker until its first error is understood. Do not attribute it to unrelated Rust changes or bypass the Swift gate.
- **References:** `scripts/test_swift.sh`, `README.md`.
