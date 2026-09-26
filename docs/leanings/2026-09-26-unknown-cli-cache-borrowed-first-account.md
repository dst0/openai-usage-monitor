# 2026-09-26 — Unknown CLI cache borrowed the first account

- **Status:** Resolved
- **Task/context:** Adversarial review of APP and CLI identity separation in the Monitor menu.
- **Unexpected observation or failure:** When the cached active CLI ID was missing or unmatched, Swift displayed the quota of the first cached account as CLI.
- **Evidence:** A synthetic status cache with an unmatched active ID and an active-looking first account loaded successfully but resolved that account as CLI. The focused loader test failed before the fix.
- **Approaches tried:**
  - **Attempt:** Fall back to `is_active`, the first account, or top-level quota.
    - **Outcome:** Rejected.
    - **Why:** Those values do not prove the current CLI credential identity when the active ID is absent or stale.
  - **Attempt:** Resolve CLI only by a matching active ID and render unknown identity as `—`.
    - **Outcome:** Worked.
    - **Why:** No unrelated quota or active badge is attributed to CLI.
- **Root cause:** The snapshot loader, model initializer, status bar, and account sections each had fallback paths that silently supplied a cached account.
- **Resolution:** Removed those fallbacks for cached snapshots and added an explicit unknown CLI display in the menu and status bar.
- **Verification:** The loader regression in `tests/CodexClientIdentityTests.swift` failed before the fix and passed after it; `tests/AppDelegateTests.swift` covers unknown CLI rendering.
- **Prevention/follow-up:** Keep identity and quota cache separate; unknown identity must remain visible as unknown.
- **Reusable learning:** A cached percentage is not a current account binding.
- **References:** `Sources/CodexClient.swift`, `Sources/QuotaModels.swift`, `Sources/AppDelegate+StatusBar.swift`, `tests/CodexClientIdentityTests.swift`.
