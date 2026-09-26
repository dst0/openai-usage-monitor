# 2026-09-26 — Stale CLI auth file kept the old quota

- **Status:** Resolved
- **Task/context:** Independent review of the APP/CLI identity fix before merging and reinstalling Codex Monitor.
- **Unexpected observation or failure:** If restoring CLI authentication after a failed Desktop checkpoint also failed, `auth.json` could hold the staged APP account while the quota cache and registry still named the previous CLI account.
- **Evidence:** The checkpoint rollback test confirmed the auth and registry divergence. A focused Swift test then showed that a cached CLI percentage still appeared after the auth file had been replaced; it failed before the reader fix.
- **Approaches tried:**
  - **Attempt:** Trust the cached `active_account_id` when it names an account in the cache.
    - **Outcome:** Rejected.
    - **Why:** The cache can outlive an atomic authentication replacement or failed rollback.
  - **Attempt:** Bind the cache to the exact auth file identity and verify matching tokens when producing it.
    - **Outcome:** Worked.
    - **Why:** Replacing or editing auth invalidates the displayed CLI identity until a new verified snapshot is written.
- **Root cause:** The cache contained a registry account ID and percentage but no proof that the current `auth.json` still belonged to that account.
- **Resolution:** Rust records file device, inode, modification time, and size only after matching live auth tokens to the active account. Swift compares this file identity before showing CLI quota; an absent or changed binding renders CLI unknown.
- **Verification:** `tests/CodexClientIdentityTests.swift` covers stale, current, and replaced auth-file identities; `tests/AppDelegateTests.swift` covers an auth replacement triggering immediate menu re-evaluation; `daemon_tick_service.test.rs` covers missing and mismatched auth. The focused identity regressions were red before the fixes and passed after them.
- **Prevention/follow-up:** Any cache that attributes a CLI quota must carry an identity tied to the current auth file; a registry ID alone is insufficient.
- **Reusable learning:** Cache freshness must be checked against the credential file that authorizes the displayed account.
- **References:** `Sources/CodexClient.swift`, `codex-switcher/src/distribution/cli_auth_file_identity_service.rs`, `codex-switcher/src/distribution/daemon_tick_service.test.rs`.
