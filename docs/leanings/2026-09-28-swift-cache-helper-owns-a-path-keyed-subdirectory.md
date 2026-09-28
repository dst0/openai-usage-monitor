# 2026-09-28 — Resolving the module cache path broke caches warmed through an alias; the scripts now own a path-keyed subdirectory

- **Status:** Resolved
- **Task/context:** Branch `fix/swift-cache-helper-hardening`, from `bc2a246`. A late adversarial review of PR #27 checked `scripts/swift_module_cache.sh` and `tests/swift_module_cache_path.sh`, introduced by [2026-09-28 — Swift module cache fails only when reused through another path spelling](2026-09-28-swift-cache-spelling-mismatch-isolated.md). All runs compiled into scratch directories only. Nothing was installed, signed, or launched, and `scripts/install.sh` was never run.
- **Unexpected observation or failure:**
  - The fix itself creates a cross-spelling reuse. The helper replaced `CLANG_MODULE_CACHE_PATH` with its physical path. A cache that another tool, or a pre-fix script run, had warmed through an alias such as `/tmp/x` was then reused as `/private/tmp/x`, which is the failing D2 case of the earlier matrix. The earlier record rejected a new cache per run because "the cost buys nothing". It does buy something whenever the scripts' spelling differs from the one that built the cache, and the fix made that the case for every alias-warmed cache.
  - A copied or moved cache fails too, even with no symlink involved.
  - A warm read-only cache, which the helper only warned about, fails as soon as a compile uses other flags. These scripts compile with several.
  - The failure message depends on the toolchain and on the source compiled, so the docs' "and crashes" and "duplicate-module error" were too narrow.
  - The regression test's recorded pre-fix failure was a `grep` failure (`does not source scripts/swift_module_cache.sh`), not a failing compile. Its ordering checks were text scans that a wrapper, an absolute path, a variable, or a later `unset` could get past.
- **Evidence:** Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`), macOS 27.2 (`26B5091g`), default SDK, a writable cache. The probe is `import Darwin` plus `getpid()`.
  - Alias-warmed cache through the helper from `bc2a246`: a raw compile through `alias/cache` passed (26 s, fresh). The helper then exported `real/cache` and the compile failed at once with `module '_DarwinFoundation1' is defined in both '<S>/alias/cache/…/_DarwinFoundation1-….pcm' and '<S>/real/cache/…'` and `cannot load underlying module for '_DarwinFoundation1'`. There was no crash for this probe.
  - The same case end to end. One `tests/ScreenContrastTests.swift` compile from `scripts/test_swift.sh`, run raw through the `/tmp` spelling of a new scratch cache, passed. `./scripts/test_swift.sh` from `bc2a246` with that `/tmp` spelling then failed with rc 1: `error: compile command failed due to signal 11` and the duplicate `_DarwinFoundation1`. The fixed script with the same variable passed every suite.
  - Copy and move, each against a cache warmed at its own path: `cp -R` 4 of 4 and `mv` 3 of 3 failed with `precompiled file '<S>/<new>/…/SwiftShims-….pcm' was compiled with module cache path '<S>/<old>/…', but the path is currently '<S>/<new>/…'` and `missing required module 'SwiftShims'`. Reuse at the original path passed.
  - Read-only warm cache (`chmod -R a-w` after one warm compile): the same compile passed 3 of 3. The same probe with `-target arm64-apple-macosx13.0` failed 3 of 3 with `unable to open output file '<S>/…/SwiftShims-….pcm': 'Permission denied'` and `could not build Objective-C module 'SwiftShims'`.
  - Failure signatures seen so far, by case:

    | Case | Toolchain | First errors | Crash |
    | --- | --- | --- | --- |
    | Second spelling, installer's `codex-ui-resume.swift` | Swift 6.4 | `module '_DarwinFoundation1' is defined in both …` | signal 11 |
    | Second spelling, `tests/ScreenContrastTests.swift` | Swift 6.4 | same | signal 11 |
    | Second spelling, `import Darwin` probe | Swift 6.4 | same, then `cannot load underlying module` | none |
    | Second spelling, same probe (PR #27 CI, `macos-14`; this branch's CI run 36364884176 in the other direction) | Swift 5.10 (`swiftlang-5.10.0.13 clang-1500.3.9.4`) | `PCH was compiled with module cache path …`, `missing required module 'SwiftShims'` | none recorded |
    | Copied or moved cache, probe | Swift 6.4 | `precompiled file … was compiled with module cache path …`, `missing required module 'SwiftShims'` | none |

  - `cd -P` then `/bin/pwd -P` on this host folds `/PRIVATE/TMP`, `/private//tmp`, `/private/./tmp`, `//private/tmp`, and `/System/Volumes/Data/private/tmp` to `/private/tmp`. It also returns the stored letter case for an upper-cased path.
- **Approaches tried:**
  - **Attempt:** Keep resolving to the physical path and document "use a new directory for a cache built through another spelling" in `README.md` too.
    - **Outcome:** Rejected.
    - **Why:** The scripts would still crash on an existing alias-warmed cache, and every reader would have to know how the cache was built.
  - **Attempt:** Give every script run a new empty cache.
    - **Outcome:** Rejected.
    - **Why:** It rebuilds every SDK module on each run. On this host the SDK interfaces come from another compiler build, so that is about 25–30 s for the probe and longer for the suites.
  - **Attempt:** Compile into a subdirectory of the physical path named `codex-monitor-swift-<POSIX cksum of the physical path>`.
    - **Outcome:** Worked.
    - **Why:** Every spelling resolves to the same physical path and so to the same subdirectory. Only the helper hands that subdirectory to the compiler, always spelled physically. Modules that other tools built in the parent are never read. A copy or move changes the physical path and so the name, which starts a new cache instead of reusing recorded paths. A constant subdirectory name would have handled aliases but not a copy or move.
  - **Attempt:** Keep accepting a warm read-only cache with a warning.
    - **Outcome:** Rejected.
    - **Why:** It only works for flags it has already seen. The subdirectory must now be writable and searchable.
  - **Attempt:** Check writability with a probe file (`mktemp` in the subdirectory, then `rm`).
    - **Outcome:** Did not work.
    - **Why:** In CI, `tests/log_permissions_and_uninstall.sh` rejected it as a new installer `mktemp` template, because `install.sh` sources the helper. The uninstaller cannot cover a probe that lives in a user-chosen cache. `[ -w ]` and `[ -x ]` (access(2)) proved equivalent here. Under a `sandbox-exec` rule denying `file-write*` on a mode-755 directory, `[ -w ]` reported it not writable, and `mktemp` failed with `Operation not permitted`. Both succeeded on a sibling the rule did not cover. `[ -x ]` catches a writable directory that cannot be searched. No file is created in the user's cache.
  - **Attempt:** Keep the grep-based ordering checks in the regression test.
    - **Outcome:** Replaced.
    - **Why:** `test_swift.sh` now runs in full under a stub `swiftc` first on `PATH` that records each call's cache path. `install.sh` cannot run in a test, so it is scanned as bash itself parses it (`declare -f` of the script wrapped in a function), which removes comments and makes nesting visible as indentation.
- **Root cause:** A module cache's `.pcm` files record the absolute paths of their imported modules in the spelling used to build them. The helper fixed the spelling that the scripts use, but it pointed them at a directory whose existing modules could have been built through any spelling.
- **Resolution:**
  - `scripts/swift_module_cache.sh` resolves the variable as before, then uses and exports the `codex-monitor-swift-<cksum>` subdirectory.
  - It rejects a newline in the value or in the resolved path, an unexpanded leading `~` (with a hint to use `${HOME}`), and a result that is not the requested directory (`-ef`). Command substitution drops a trailing newline, which could otherwise name a sibling. It also rejects a subdirectory that is a symlink or not a directory, a failed checksum, and a subdirectory that is not writable and searchable (`[ -w ]`, `[ -x ]`).
  - Its info line now goes to stderr.
  - `CODEX.md`, `README.md`, and `AGENTS.md` describe the subdirectory, the per-toolchain signatures, and the rule for other tools that share a cache.
- **Verification:**
  - `tests/swift_module_cache_path.sh` compiles first. It warms a cache raw through an alias, and asserts that raw reuse through the physical spelling fails with a recognized signature (`is defined in both`, `was compiled with module cache path`, or `missing required module 'SwiftShims'`). A toolchain that compiles it fails the test, because the rest would prove nothing there.
  - It then compiles through the helper given the alias and given the physical spelling. Both compiles must pass before any value is compared. Inode, mtime, and size of every `.pcm` and `.swiftmodule` must not change on the second compile.
  - In CI run 36364884176 (`macos-14`, Swift 5.10), the control reported `was compiled with module cache path` and both helper compiles passed, in the run nested in `test_swift.sh` and in the dedicated step.
  - Its helper cases also run the helper under a `sandbox-exec` rule that denies writes to a mode-755 cache, which must be rejected (skipped where `sandbox-exec` cannot run), and give it a mode-600 subdirectory.
  - Against the `bc2a246` helper it fails at the first helper compile with the duplicate `_DarwinFoundation1`. With the helper reduced to `return 0` it fails at the second, with the same error.
  - Nineteen single-edit mutations of the helper, run on scratch copies, each failed the test: dropping the `./` prefix, either newline check, `-ef`, the writability check or its probe-file cleanup (an earlier version), the symlink or file check, or the checksum check; `pwd -L`; keying by the requested spelling; no subdirectory; stdout output; returning 0 on a rejection; and others. A twentieth, dropping `CDPATH=''`, passed: the `./` prefix already keeps `cd` away from `CDPATH`, so the redundant override was removed. Thirteen mutations of `test_swift.sh`, `install.sh`, and a sourced helper also failed it: a compile before the call, the call removed or nested in a block, `xcrun swiftc`, `"${SWIFTC}"`, `/usr/bin/swiftc`, an assignment prefix, a `cd` before the call, an unset or reassignment after it, and an unchecked `source`. The scan also checks itself on sample scripts.
  - `CLANG_MODULE_CACHE_PATH=<new scratch dir> ./scripts/test_swift.sh` passed (159 s), and passed again through the `/tmp` spelling of the same directory (102 s), using the same subdirectory. The regression test alone takes 44–66 s here, most of it two cold warm-ups.
- **Prevention/follow-up:**
  - The subdirectory makes the rule structural for these scripts. Other tools that share one cache must still use one spelling and must not reuse a copied or moved cache.
  - Old `codex-monitor-swift-*` directories are left after a move; the docs say they can be deleted.
  - The CI control now fails if a runner toolchain stops reproducing the hazard. Re-verify then, rather than relaxing the assertion.
  - Caching the test's own warm-up across runs was rejected: its deliberately failing control writes into that cache.
  - The installer's ordering is still checked statically, not by running it.
  - `install.sh` sources the helper, so the helper counts as installer code for `tests/log_permissions_and_uninstall.sh`. Run every `tests/*.sh` locally before pushing a change to a sourced script.
- **Reusable learning:** A fix that rewrites a path must consider state already built under the old spelling. Give a tool its own subdirectory keyed by the physical path when the parent may hold artifacts built through other spellings or at other locations. Make a regression test fail on the behaviour (a compile's exit status) before comparing strings, and assert that its control still reproduces the hazard.
- **References:** `scripts/swift_module_cache.sh`, `tests/swift_module_cache_path.sh`, `scripts/test_swift.sh`, `scripts/install.sh`, `CODEX.md`, `README.md`, `AGENTS.md`, [2026-09-28 — Swift module cache fails only when reused through another path spelling](2026-09-28-swift-cache-spelling-mismatch-isolated.md), [2026-09-28 — Swift's "SDK is not supported" error came from an unwritable module cache](2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md), [2026-09-28 — Evidence for the module-cache records lived only in PR #27](2026-09-28-swift-cache-evidence-lived-only-in-the-pr.md).
