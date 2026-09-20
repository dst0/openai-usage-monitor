# 2026-09-20 — persistent output needs one redaction boundary

- **Status:** Resolved
- **Task/context:** Hardening Codex Monitor's switcher log and launchd daemon/restart-worker output.
- **Unexpected observation or failure:** File permissions and no-follow opens protected the log destinations, but messages written through the logger, distribution audit path, and inherited worker streams still had separate redaction behavior. A display name or account reference could therefore survive in a persistent line even when an email or path was masked.
- **Evidence:** Focused tests reproduced raw identity, UUID, path, token, argv, control-character, and quoted JSON values at the message boundary; the logger and audit logger did not initially share one formatter.
- **Approaches tried:**
  - **Attempt:** Keep the existing audit sanitizer and rely on the logger to clean later.
    - **Outcome:** Did not work.
    - **Why:** Audit entries and launchd streams can be observed independently of a later logger call.
  - **Attempt:** Route persistent messages and background output through one named service while retaining direct state/journal fields.
    - **Outcome:** Worked.
    - **Why:** Identity fields now become deterministic namespaced references, paths/tokens/argv use constant markers, and operational reason/phase/status fields remain readable without changing recovery state.
- **Root cause:** Output sinks had different ownership and formatting paths; protecting the file alone did not protect the content emitted before or beside that file.
- **Resolution:** Added `LogRedactionService`, connected it to `logger::log`, audit events, logger macros, daemon output, and restart-worker/recovery output. `DistributionAuditLogger` now uses the same fd-anchored append service as the main logger.
- **Verification:** Redaction, audit/logger parity, Brotli roundtrip, distribution correlation, focused logger tests, and the full functional Rust suite passed. The repository file-limit test still reports unrelated pre-existing oversized files; the new redaction source is within the 300-line limit.
- **Prevention/follow-up:** Keep persistent output structured with explicit identity keys and route new background or launchd-visible messages through `LogRedactionService`; do not redact the 0600 recovery journal/state fields needed to resume work.
- **Reusable learning:** A private log file is not a private log contract; every persistent sink and inherited background stream needs the same content redaction boundary.
- **References:** `codex-switcher/src/distribution/log_redaction_service.rs`; `codex-switcher/src/distribution/log_redaction_output.rs`; `codex-switcher/src/distribution/distribution_audit_logger.rs`; `codex-switcher/src/logger.rs`.
