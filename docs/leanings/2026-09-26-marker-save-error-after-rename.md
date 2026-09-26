# 2026-09-26 — A marker save error may follow successful rename

- **Status:** Resolved
- **Task/context:** Reviewing offline account distribution rollback around the Desktop session marker.
- **Unexpected observation or failure:** The marker writer can return an error if directory sync fails after an atomic rename. A caller that treats every save error as pre-write can restore auth and registry while leaving the target marker behind, then clear the journal.
- **Evidence:** A deterministic post-rename failure hook leaves the attempted marker at the path before rollback. A full offline commit test exercises auth write, registry commit, marker failure, all three restorations, and journal cleanup with synthetic accounts.
- **Approaches tried:**
  - **Attempt:** Roll back only auth and registry on marker save failure.
    - **Outcome:** Did not work.
    - **Why:** The marker may already name the target account.
  - **Attempt:** Load the prior marker before the transaction and restore it only if the observed marker matches this attempt; retain the journal on mismatch.
    - **Outcome:** Worked in focused tests.
    - **Why:** A post-rename failure cannot leave a committed marker paired with rolled-back credentials while reporting clean rollback.
- **Root cause:** Save failure was assumed to imply no visible filesystem mutation.
- **Resolution:** Offline rollback restores and verifies the prior marker before clearing the distribution journal. Relaunch binding also attempts restoration on save error.
- **Verification:** Four marker I/O tests, five offline commit tests, and 16 distribution transaction safety tests pass. The integrated post-rename test verifies the previous auth, active account, and marker and the cleared journal. Real filesystem sync failure was injected rather than forced on a live credential file.
- **Prevention/follow-up:** Treat atomic replacement plus directory durability as two observable phases. A same-user uncooperative pathname race remains outside the Monitor's cooperative lock.
- **Reusable learning:** A write returning an error after rename still requires rollback of the file it may have replaced.
- **References:** `codex-switcher/src/distribution/desktop_app_session.rs`, `codex-switcher/src/distribution/distribution_offline_commit_service.rs`, and their adjacent test files.
