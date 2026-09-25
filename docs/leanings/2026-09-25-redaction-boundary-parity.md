# 2026-09-25 — Redaction at malformed and list-valued boundaries

- **Status:** Resolved
- **Task/context:** Bring shared OpenAI and AGY Monitor log sanitizers into parity while reviewing recent cross-project changes.
- **Unexpected observation or failure:** Incomplete quoted fields, multiword Authorization values, wrapped credentials, and later email or UUID values in a list could survive sanitization.
- **Evidence:** Three synthetic regression tests failed on the prior OpenAI implementation and pass after the patch. No live credentials or customer data were used.
- **Approaches tried:**
  - **Attempt:** Match only closed quotes and the first sensitive value in a token.
    - **Outcome:** Did not work.
    - **Why:** Truncated helper output and comma-joined identifiers left later values visible.
  - **Attempt:** Treat unquoted Authorization as a single word.
    - **Outcome:** Did not work.
    - **Why:** Scheme and credential are separated by whitespace.
- **Root cause:** The parser required a closing quote and the token scanner returned after the first match.
- **Resolution:** Redact incomplete quoted values through the end of the diagnostic, consume the full unquoted Authorization value, detect credentials inside wrappers, and replace every email and UUID occurrence.
- **Verification:** `cargo test distribution::log_redaction_service_tests --bin codex-mon` passes all nine focused tests after the change; full repository gates are rerun before publication.
- **Prevention/follow-up:** Keep malformed, multiword, and list-valued samples in the shared Monitor regression suites.
- **Reusable learning:** A log sanitizer must fail closed on incomplete delimiters and replace every sensitive occurrence in each token.
- **References:** `codex-switcher/src/distribution/log_redaction_service.test.rs`, `codex-switcher/src/distribution/log_redaction_structured_parser.rs`, `codex-switcher/src/distribution/log_redaction_token_service.rs`.

## 2026-09-25 final review update

The three failing cases and nine passing cases above describe the first focused checkpoint. Subsequent adversarial review reproduced and fixed delimiter-joined tails, escaped nested keys, malformed quote suffixes, escaped opening quotes, and unbounded scans of long tokens or nested operational fields. A checked slice handles a terminal backslash without panic. The final focused suite contains 18 passing synthetic tests. Ambiguous quoted secrets are hidden through the end of the diagnostic, which can also hide later operational fields on that line.

### Multiword boundary follow-up

A later review reproduced unquoted multiword password and display-name leaks. The parser now hides the rest of an ambiguous diagnostic after such fields and retains a single-token boundary only for validated thread or turn UUIDs. The focused suite now contains 19 passing synthetic tests. This conservative rule can hide later operational fields on a line.

The first full Rust run exposed two test fixtures that expected later path markers after an unquoted sensitive value. Those fixtures now use quoted structured fields, which provide real boundaries and still verify historical-file and Brotli-log redaction. The focused tests retain the unquoted multiword cases so this change does not weaken that privacy contract.
