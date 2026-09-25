# 2026-09-25 — Swift SDK/compiler mismatch blocked the local release gate

- **Status:** Open
- **Task/context:** Validating the Codex Monitor cold task recovery change before reinstalling the macOS application.
- **Unexpected observation or failure:** `./scripts/test_swift.sh` failed while building its first suite, before any changed Swift source was compiled.
- **Evidence:** `xcode-select -p` selected `/Library/Developer/CommandLineTools`. The active compiler reported Swift `6.4.0.34.1`; the selected macOS SDK Swift interface reported `6.4.0.31.4`. The compiler rejected the SDK interface. The sandbox also denied writing the default Clang module cache; the version mismatch is independently decisive.
- **Approaches tried:**
  - **Attempt:** Run the repository's canonical Swift test script.
    - **Outcome:** Did not work.
    - **Why:** Local Command Line Tools compiler and SDK versions differ.
- **Root cause:** The selected local Command Line Tools installation is internally inconsistent. The change does not touch Swift sources.
- **Resolution:** The Rust gate passed, but local Swift release validation and reinstall remain pending until a matching Apple toolchain is selected or repaired.
- **Verification:** `xcrun swiftc --version`, `xcode-select -p`, and the first Swift test failure establish the mismatch; the Swift suite has not passed in this checkout.
- **Prevention/follow-up:** Require a matching Swift compiler/SDK and rerun `./scripts/test_swift.sh` before installing, signing, or promoting the application. The README prerequisites now state this gate.
- **Reusable learning:** Treat an SDK/compiler interface version mismatch as an environment release blocker; do not attribute it to unrelated Rust changes or bypass the Swift gate.
- **References:** `scripts/test_swift.sh`, `README.md`.
