# 2026-09-26 — Keep automatic switching disabled while cold recovery is unproven

- **Status:** Resolved
- **Task/context:** Preserve the user's disabled auto-switch setting while the installed Desktop's cold-task recovery and multiwindow restoration remain unverified.
- **Unexpected observation or failure:** A fresh registry and a registry missing `auto_switch_enabled` both defaulted the setting to `true`.
- **Evidence:** The `Settings` default constructor and serde field default were `true`; a regression checks fresh, legacy, and explicitly enabled settings.
- **Approaches tried:**
  - **Attempt:** Rely on the currently installed registry containing `false`.
    - **Outcome:** Did not work as a general guard.
    - **Why:** A fresh install or missing field would silently enable switching.
  - **Attempt:** Default to `false` while preserving explicit user opt-in.
    - **Outcome:** Worked in the regression test.
    - **Why:** Both constructor and deserialization now choose `false`, while an explicit `true` remains honored.
- **Root cause:** Automatic switching was opt-out in both default paths despite an unproven cold recovery contract.
- **Resolution:** New and legacy registries default to disabled. Existing explicit configuration remains user controlled.
- **Verification:** `new_and_legacy_registries_keep_automatic_switching_disabled` passes.
- **Prevention/follow-up:** Do not change the default until cold-task mounting and selected-task restoration have live evidence for the installed Desktop build.
- **Reusable learning:** Safety-sensitive automation should require explicit opt-in when its recovery path is unproven.
- **References:** `codex-switcher/src/models/settings.rs`, `codex-switcher/src/models/settings.test.rs`.
