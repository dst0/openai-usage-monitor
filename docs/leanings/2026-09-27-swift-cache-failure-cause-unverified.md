# 2026-09-27 — Swift module-cache failure cause is unverified

- **Status:** Partial
- **Task/context:** Review finding on PR #15 against [2026-09-27 — Swift module cache alias interrupted Monitor installation](2026-09-27-swift-cache-symlink-alias-breaks-install.md) and the matching guidance in `CODEX.md` and `README.md`.
- **Unexpected observation or failure:** The earlier learning blames the duplicate `_DarwinFoundation1` failure on the `/tmp` symlink alias and states that as `Resolved`. Its two recorded attempts do not isolate that cause.
- **Evidence:** In the earlier record, the failing attempt reused an existing writable cache reached through `/tmp`. The successful attempt used a fresh cache under canonical `/private/tmp`. Both variables changed together: cache contents (reused or empty) and path spelling (alias or canonical). The duplicate-definition message named both spellings. That is consistent with aliasing, but also with a reused cache that already held modules built under the other spelling or for a different SDK on a host with a known compiler/SDK build mismatch.
- **Approaches tried:**
  - **Attempt:** Re-read the recorded attempts to see whether either variable was held constant.
    - **Outcome:** Did not work.
    - **Why:** Neither run held one variable fixed, so the evidence supports only the combined workaround.
  - **Attempt:** Run a controlled comparison in this change.
    - **Outcome:** Not attempted.
    - **Why:** This review fix was limited to documentation and did not run Swift builds or installs, or touch the installed Monitor. The toolchain state that produced the failure may also have changed since.
- **Root cause:** Unconfirmed. Candidates: the alias path spelling alone, stale cache contents from an earlier build (other spelling or other SDK), or both together.
- **Resolution:** `CODEX.md` and `README.md` now give only the verified workaround: a new, empty cache directory under canonical `/private/tmp` for each install. The alias explanation is marked unverified. The earlier learning is kept unchanged as the historical record.
- **Verification:** Documentation-only change; no build or install was run.
- **Prevention/follow-up:** In an isolated scratch build (not an install, not the installed app), compile one Swift helper with the same SDK override four ways: fresh `/tmp` cache, fresh `/private/tmp` cache, reused `/tmp` cache, and reused `/private/tmp` cache. Each reused cache should be pre-populated through the other spelling. Record the result in a new learning that links this one. Update the guidance only if one variable is isolated.
- **Reusable learning:** A retry that changes two variables proves only that the combination works. Record which variable caused the failure as a hypothesis until a controlled comparison isolates it.
- **References:** `docs/leanings/2026-09-27-swift-cache-symlink-alias-breaks-install.md`, `CODEX.md`, `README.md`.
