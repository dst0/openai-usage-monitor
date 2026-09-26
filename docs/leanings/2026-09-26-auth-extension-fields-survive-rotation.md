# 2026-09-26 — Auth extension fields survive rotation

- **Status:** Resolved
- **Task/context:** Re-login an account while sharing `auth.json` with the official ChatGPT Desktop.
- **Unexpected observation or failure:** Parsing the file into a narrow `AuthJson` model discarded unknown top-level and token fields; subsequent comparison and replacement could miss a Desktop metadata change or erase that metadata.
- **Evidence:** `compare_write_preserves_unknown_auth_and_token_fields` and `compare_write_rejects_unknown_auth_field_change_after_staging` exercise preservation and conflicting-change rejection with isolated JSON fixtures. `active_relogin_preserves_unknown_token_metadata_in_auth_and_registry` covers the registry copy.
- **Approaches tried:**
  - **Attempt:** Compare only known token and account fields.
    - **Outcome:** Did not work
    - **Why:** Desktop may add fields that the Monitor model has not yet named.
  - **Attempt:** Flatten unknown fields into the model and retain a raw JSON snapshot for the final content comparison.
    - **Outcome:** Worked
    - **Why:** The replacement keeps extension fields and a late change to any field rejects the commit.
- **Root cause:** A lossy typed parse was treated as the complete shared auth document.
- **Resolution:** `AuthJson` and `AuthTokens` carry unknown fields, active auth replacement preserves them, and compare/write checks the raw document again immediately before rename.
- **Verification:** The focused regressions and full `cargo test --quiet --bin codex-mon` suite passed 344 tests before the final staging-file hardening. No live auth file was modified.
- **Prevention/follow-up:** Keep extension-field regressions and full-document readback whenever the official Desktop may change its auth format.
- **Reusable learning:** A shared credential format must be compared and rewritten without dropping fields the current client does not understand.
- **References:** `codex-switcher/src/models/auth_json.rs`, `codex-switcher/src/models/auth_tokens.rs`, `codex-switcher/src/storage/active_auth_compare_write_service.rs`.
