# 2026-09-26 — System process rows are valid in Desktop probes

- **Status:** Resolved
- **Task/context:** Validate fail-closed ChatGPT process discovery before account switching.
- **Unexpected observation or failure:** A switch test could not reach its re-login guard because process inspection rejected the full `/bin/ps` result.
- **Evidence:** A focused regression with ordinary `kernel_task` PID 0 and `launchd` PID 1 rows failed with `Desktop process inspection returned an invalid process`. The same parser is used by the checked main-process and bundled-writer probes.
- **Approaches tried:**
  - **Attempt:** Reject PID 0 and PID 1 while parsing every process row.
    - **Outcome:** Did not work.
    - **Why:** These are normal system rows, even though neither is a ChatGPT process.
  - **Attempt:** Parse valid system rows, then reject PID 0 or 1 only if an exact ChatGPT executable match carries one.
    - **Outcome:** Worked.
    - **Why:** Unrelated system rows no longer invalidate the inspection, while an impossible target identity still fails closed.
- **Root cause:** The probe applied a target-process PID invariant to unrelated rows from the whole process table.
- **Resolution:** Moved the PID threshold check to exact ChatGPT matches and kept strict parsing of malformed rows.
- **Verification:** The focused `parses_only_the_exact_codex_app_executable` regression passed after failing before the fix; full integration validation remains part of this change.
- **Prevention/follow-up:** Include real system process rows in process-probe fixtures and validate the complete switch path.
- **Reusable learning:** Parse a broad system inventory before applying rules that belong only to selected target records.
- **References:** `codex-switcher/src/switcher/codex_process_probe.rs`, `codex-switcher/src/switcher.test.rs`.
