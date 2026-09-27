# 2026-09-28 — Swift module cache fails only when reused through another path spelling

- **Status:** Resolved
- **Task/context:** Follow-up to the `Partial` record [2026-09-27 — Swift module-cache failure cause is unverified](2026-09-27-swift-cache-failure-cause-unverified.md), which reviewed [2026-09-27 — Swift module cache alias interrupted Monitor installation](2026-09-27-swift-cache-symlink-alias-breaks-install.md). A controlled comparison compiled the exact `swiftc` invocations of `scripts/install.sh` (four compiles) and `scripts/test_swift.sh` into scratch directories only. Nothing was installed, signed, or launched, and the repository's `build/` was not touched.
- **Unexpected observation or failure:** Reusing a warm module cache through the other spelling of the same directory fails every time, in both directions. Reusing it through the spelling that built it always works, and so does a fresh cache under either spelling. The Swift compiler/SDK version difference is not needed for the failure. Plain `clang -fmodules` fails the same way.
- **Evidence:**
  - Toolchain, unchanged since 2026-09-14: all `com.apple.pkg.CLTools_*` packages are `27.0.0.0.*` with that install time. `swiftc` is Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`). The default SDK is macOS 27.0 (Swift interfaces from `6.4.0.31.4`); `MacOSX26.5.sdk` has interfaces from `6.3.2.1.2`. macOS 27.2 (`26B5091g`).
  - Incident, from the local agent transcript: the cache `/private/tmp/codex-monitor-clang-cache` had been built and reused only through `/private/tmp`, always with `SDKROOT=.../MacOSX26.5.sdk`, by `test_swift.sh`, `install.sh`, and ad hoc `swiftc` runs on 2026-09-25. On 2026-09-26 an install passed it as `CLANG_MODULE_CACHE_PATH=/tmp/codex-monitor-clang-cache`, still with the 26.5 SDK. `swift-frontend` then compiled `scripts/codex-ui-resume.swift` and crashed with signal 11 after `module '_DarwinFoundation1' is defined in both '/private/tmp/.../38KI8GH79R04T/_DarwinFoundation1-29KYSLY2NX7JV.pcm' and '/tmp/.../38KI8GH79R04T/_DarwinFoundation1-29KYSLY2NX7JV.pcm'`. Both paths name one file. The frontend received no `-module-cache-path`, so the path came from the environment variable.
  - Matrix, with `SDKROOT=.../MacOSX26.5.sdk` and a new cache directory for every chain. Each chain built a cache with the four install compiles, then optionally reused it with the same four compiles:

    | Cell | Build spelling | Reuse spelling | Runs | Result |
    | --- | --- | --- | --- | --- |
    | A: fresh | `/tmp` | — | 6 | all 4 compiles passed |
    | B: fresh | `/private/tmp` | — | 7 | all 4 compiles passed |
    | C1: warm, same | `/tmp` | `/tmp` | 3 | all 4 compiles passed |
    | C2: warm, same | `/private/tmp` | `/private/tmp` | 3 | all 4 compiles passed |
    | D1: warm, cross | `/private/tmp` | `/tmp` | 4 | failed at compile 1: duplicate `_DarwinFoundation1`, signal 11 |
    | D2: warm, cross | `/tmp` | `/private/tmp` | 3 | failed at compile 1: duplicate `_DarwinFoundation1`, signal 11 |
    | F0: `-module-cache-path`, fresh | `/tmp` | — | 3 | all 4 compiles passed |
    | F1: `-module-cache-path`, cross | `/private/tmp` | `/tmp` | 3 | failed at compile 1: duplicate `_DarwinFoundation1`, signal 11 |
    | F2: `-module-cache-path`, same | `/tmp` | `/tmp` | 3 | all 4 compiles passed |
    | G1: built by test and install compiles, same | `/private/tmp` | `/private/tmp` | 3 | all 4 compiles passed |
    | G2: built by test and install compiles, cross | `/private/tmp` | `/tmp` | 3 | failed at compile 1: duplicate `_DarwinFoundation1`, signal 11 |

    A counts the build step of the C1 and D2 chains, and B the build step of the C2 and D1 chains plus one pilot run. The D1 count includes that pilot. The first error line of every failing run was `error: compile command failed due to signal 11 (use -v to see invocation)`. The next line named the same `.pcm` under both spellings. The stack trace matched the incident, down to type-checking `findCodexTargetPID()`.
  - Mechanism: a built `.pcm` stores the absolute paths of the module files it imports, in the spelling used when it was built. For example, `_DarwinFoundation2.pcm` stores `/private/tmp/.../_DarwinFoundation1-….pcm`. A compile that uses the other spelling looks the direct import up under its own spelling, then follows the stored spelling from a dependent module, and loads one module under two names.
  - Independence from the Swift SDK difference and from `/tmp`: a symlink created in scratch (`alias -> real`) reproduced the failure for `import Darwin`. Apple clang 2100.3.34.2 with `-fmodules -fmodules-cache-path` failed the same way 6 of 6 times, 3 with the 26.5 SDK and 3 with the default 27.0 SDK: `module '_DarwinFoundation2' is defined in both '.../alias/...pcm' and '.../real/...pcm'`. The same-spelling reuse after each failure passed.
  - Another toolchain: in CI for PR #27, on GitHub's `macos-14` runner with its Xcode toolchain, the regression test's raw-alias control also failed. It reported `PCH was compiled with module cache path '…/real/cache/…', but the path is currently '…/alias/cache/…'` and `missing required module 'SwiftShims'`. The compile through the resolved path passed.
- **Approaches tried:**
  - **Attempt:** Vary one factor at a time: path spelling at build and at reuse, fresh or warm cache, environment variable or `-module-cache-path`, and which compiles built the cache.
    - **Outcome:** Worked.
    - **Why:** Only a spelling mismatch between build and reuse failed. Reuse alone, a cache seconds old, and a cache built by different compile sets all passed when the spelling matched.
  - **Attempt:** Pass the cache with `swiftc -module-cache-path` instead of `CLANG_MODULE_CACHE_PATH`.
    - **Outcome:** Did not work as a fix.
    - **Why:** The driver passes the flag through without resolving symlinks, so cross-spelling reuse fails the same way (F1).
  - **Attempt:** Give every script run a new, empty cache.
    - **Outcome:** Rejected.
    - **Why:** It avoids the hazard but rebuilds every SDK module on each run (about 60–200 s on this host). No failure was ever tied to a same-spelling cache, so the cost buys nothing.
  - **Attempt:** Resolve `CLANG_MODULE_CACHE_PATH` to its physical path in the scripts before the first `swiftc`.
    - **Outcome:** Worked.
    - **Why:** Every script run builds and reuses the cache through one spelling, so the incident case (build `/private/tmp`, reuse `/tmp`) becomes the passing C2 case.
- **Root cause:** Clang's implicit module cache records imported module files by the absolute path spelling used when it was built. Reusing it through another spelling of the same directory loads one module file under two names. Clang 2100 rejects that as a duplicate definition, and Swift 6.4's `swift-frontend` then crashes. The older toolchain on GitHub's `macos-14` runner instead rejects the mismatched module cache path recorded in the file. The `/tmp` symlink was one such spelling; a stale cache and the compiler/SDK version difference were not required.
- **Resolution:** New `scripts/swift_module_cache.sh` (`canonicalize_clang_module_cache_path`). `scripts/install.sh` and `scripts/test_swift.sh` source it and call it fail-closed before their first `swiftc`. An unset or empty variable is left alone. A set one is created if missing and replaced by its physical path, which also pins a relative path to the caller's directory. A path that cannot be a directory, or that is not writable, stops the script. `CODEX.md` and `README.md` now state the same-spelling rule instead of requiring a new cache for each install.
- **Verification:**
  - `tests/swift_module_cache_path.sh`, run from `./scripts/test_swift.sh` and so in CI, failed before the fix (`scripts/install.sh: does not source scripts/swift_module_cache.sh`) and passes after it.
  - The test checks that both scripts call the helper before their first `swiftc`. It covers the helper for unset, empty, physical, aliased, missing, trailing-slash, relative-with-`CDPATH`, `-`, leading-dash, spaced, regular-file, symlink-to-file, read-only, and `/tmp` paths.
  - It also warms a real cache, then compiles through an alias after resolution. That compile must pass and must add no `.pcm` files. A final informational control reuses the raw alias; on this host it reported `raw alias reuse reproduced the duplicate-module failure`.
  - End to end: the pre-fix `./scripts/test_swift.sh` (snapshot of `f88f8ca`) built a cache through `/private/tmp`. The same pre-fix script then failed through `/tmp`, and the fixed script passed through `/tmp`. Exact results are in the PR.
- **Prevention/follow-up:** The scripts now enforce one spelling. Tools outside them (ad hoc `swiftc`, `swift`, or `clang -fmodules`) that share a cache must also pass the physical path. A cache built through a different spelling from the one a script will resolve to must be replaced with a new directory. Cache age was not varied. The incident cache was about a day old, but a cache minutes old reproduced the failure, so age is not required.
- **Reusable learning:** Reuse a Clang or Swift module cache only through the exact path spelling that built it. Resolve the path with `cd -P`/`pwd -P` before handing it to the compiler, and treat a retry that changes two variables as unproven until one is isolated.
- **References:** `scripts/swift_module_cache.sh`, `tests/swift_module_cache_path.sh`, `scripts/install.sh`, `scripts/test_swift.sh`, `CODEX.md`, `README.md`, [2026-09-27 — Swift module cache alias interrupted Monitor installation](2026-09-27-swift-cache-symlink-alias-breaks-install.md), [2026-09-27 — Swift module-cache failure cause is unverified](2026-09-27-swift-cache-failure-cause-unverified.md), [2026-09-28 — Swift's "SDK is not supported" error came from an unwritable module cache](2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md).
