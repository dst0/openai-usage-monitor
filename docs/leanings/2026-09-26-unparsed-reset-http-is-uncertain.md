# 2026-09-26 — Unparsed reset HTTP is uncertain

- **Status:** Resolved
- **Task/context:** Review service outcome classification used by manual and automatic reset-credit journals.
- **Unexpected observation or failure:** An unparsed HTTP 4xx response was classified as definite non-consumption, resolving the manual journal and allowing a later request with a new key.
- **Evidence:** A synthetic local HTTP endpoint returned malformed 408, 409, 429, and 400 responses. The 408 case failed the `Unknown` assertion before the fix; all four pass after it. No external service was called.
- **Approaches tried:**
  - **Attempt:** Infer non-consumption from any status below 500.
    - **Outcome:** Did not work.
    - **Why:** The status alone does not establish whether a request reached or was applied by the reset service.
  - **Attempt:** Accept only parsed service codes as authoritative and retain every unparsed HTTP status as unknown.
    - **Outcome:** Worked in synthetic tests.
    - **Why:** Unknown outcomes keep the original idempotency key and block a fresh manual request.
- **Root cause:** The HTTP transport status was treated as a definitive business outcome without an authoritative response body.
- **Resolution:** The reset client maps an unparsed status at any code to `Unknown`; parsed `reset`/`already_redeemed` and explicit non-consumption codes retain their previous meanings.
- **Verification:** Five quota tests pass, including the red-to-green 4xx regression.
- **Prevention/follow-up:** Preserve uncertain outcomes in the journal and confirm terminal results from a parsed service code or official account evidence.
- **Reusable learning:** A transport status is not an idempotent business result; do not mint another key from an unparsed error.
- **References:** `codex-switcher/src/quota/reset_credit_consumption.rs`, `codex-switcher/src/quota.test.rs`, `README.md`.
