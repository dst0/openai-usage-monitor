# 2026-09-28 — Concurrent Swift suite runs share the AppDelegate test's defaults

- **Status:** Open
- **Task/context:** While checking that the default SDK passes `./scripts/test_swift.sh` for [2026-09-28 — Swift's "SDK is not supported" error came from an unwritable module cache](2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md), two full suite runs were started at the same time from one checkout. Each had its own module cache.
- **Unexpected observation or failure:** One run failed in the AppDelegate suite with `❌ Assertion Failed: condition is false - Default stackPercentages must be true at tests/AppDelegateTests.swift:253`. The other run passed. Runs alone, before and after, passed.
- **Evidence:** `tests/AppDelegateTests.swift` (Test 4, lines 251–259) calls `UserDefaults.standard.removeObject(forKey: "stackPercentages")`, reads the key back, then writes `false` and `true`. The test binary has no bundle, so `UserDefaults.standard` is the `app_delegate_test` preferences domain (`defaults read app_delegate_test` shows `stackPercentages`). Every concurrent run on the machine shares that domain, from any checkout or worktree. The failure fits the other run writing `false` between this run's remove and its read.
- **Approaches tried:**
  - **Attempt:** Run the two suites in parallel to save time.
    - **Outcome:** Did not work.
    - **Why:** Both processes changed and read the same per-user preferences key without isolation.
- **Root cause:** Leading hypothesis: test state in a machine-wide preferences domain shared by concurrent runs. A deterministic reproduction (a background writer toggling the key while the suite runs) has not been done. That is the evidence still missing for `Resolved`.
- **Resolution:** None in this change. Run `./scripts/test_swift.sh` one at a time per user account until the test uses an isolated suite, for example `UserDefaults(suiteName:)` with a unique name that is removed afterwards.
- **Verification:** The failing log and a clean single rerun are recorded in the PR that added this file.
- **Prevention/follow-up:** Isolate the AppDelegate test's defaults and add a check that a concurrent writer cannot change the result. Several agents run worktrees of this repository on one host at the same time, so this can surface as an unrelated flaky failure.
- **Reusable learning:** Tests that touch `UserDefaults.standard` from a bundle-less binary share one per-user domain across every concurrent run. Give each run its own suite before running suites in parallel.
- **References:** `tests/AppDelegateTests.swift`, `scripts/test_swift.sh`.
