# 2026-09-26 — Re-login workspace is not user identity

- **Status:** Partial
- **Task/context:** Repair browser re-login of a saved ChatGPT account in Codex Monitor.
- **Unexpected observation or failure:** Incoming tokens without a readable email claim were accepted when their workspace ID matched the saved account. A workspace can contain more than one user, so this could replace the selected user's stored credentials with another user's login.
- **Evidence:** `relogin_same_workspace_rejects_tokens_without_a_provable_email` failed against the old implementation: `apply_relogin_to_accounts_file` returned success and changed the staged entry. No live credentials were used.
- **Approaches tried:**
  - **Attempt:** Treat matching workspace IDs as sufficient identity proof.
    - **Outcome:** Did not work
    - **Why:** Workspace membership does not identify the human account.
  - **Attempt:** Require a usable email claim from the official CLI browser login and an exact match to the saved email before staging tokens.
    - **Outcome:** Worked for the covered missing and mismatched claim cases.
    - **Why:** A token set lacking that identity now fails without modifying the registry.
- **Root cause:** The previous check only rejected two known, differing email strings; absent or unparseable email took the acceptance path.
- **Resolution:** Re-login now requires a usable existing and incoming email identity, rejects conflicting ID/access token email claims, and checks workspace separately. Missing or placeholder incoming workspace IDs cannot replace an existing non-default workspace. The JWT claims are decoded locally from tokens produced by the official CLI; no local signature verification is claimed.
- **Verification:** The full `cargo test --quiet --bin codex-mon` suite passed 344 tests after the identity fix, including missing-email, conflicting-email, and missing-workspace regressions. Signed installation and live browser re-login were not attempted in this isolated test.
- **Prevention/follow-up:** Keep the missing-email, conflicting-claim, and workspace-mismatch regressions. Re-login of an old placeholder-email registry entry requires account re-registration with a known email. Recheck the identity contract if the official CLI changes its token format.
- **Reusable learning:** A shared workspace ID never substitutes for a user identity when replacing stored credentials.
- **References:** `codex-switcher/src/setup/relogin_service.rs`, `codex-switcher/src/setup.test.rs`.
