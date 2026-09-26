# 2026-09-26 — Validate Accessibility value types before window geometry reads

- **Status:** Resolved
- **Task/context:** Inspect ChatGPT windows before a Desktop account switch without crashing the exact-process window helper.
- **Unexpected observation or failure:** The helper force-cast each Accessibility geometry response to `AXValue` and passed an unconstrained generic output buffer to `AXValueGetValue`. A provider returning another CF type could terminate the helper, and a payload type mismatch could make the geometry read invalid.
- **Evidence:** The prior `axValue<T>` implementation used `as! AXValue` without checking `CFGetTypeID` or `AXValueGetType`. A regression test prepared valid point/size values, swapped payload types, and ordinary number/string objects. Before the decoder was added, its test target failed to compile because the safe decode functions were absent.
- **Approaches tried:**
  - **Attempt:** Keep a generic decoder and rely on `AXValueGetValue` to reject mismatches.
    - **Outcome:** Rejected.
    - **Why:** A generic output type can have an incompatible buffer size, and the forced cast can trap before the API checks the payload type.
  - **Attempt:** Use dedicated point and size decoders.
    - **Outcome:** Worked.
    - **Why:** Each decoder validates the CF type, exact AX payload type, read result, and finite coordinates before returning typed geometry.
- **Root cause:** The window helper trusted an external Accessibility provider's value type.
- **Resolution:** The helper now uses `CodexWindowAXValueDecoder.swift` for point and size reads. Installer and Swift test builds compile the same decoder source.
- **Verification:** The dedicated Swift regression passed for valid geometry, negative screen origins, wrong AX payload types, non-AX objects, and non-finite coordinates when accepted by `AXValueCreate`. The optimized window helper built successfully. No live malformed Accessibility response was injected.
- **Prevention/follow-up:** Preserve typed decoding on every new AX geometry read and run `tests/CodexWindowRestoreAXValueTests.swift` through `scripts/test_swift.sh`.
- **Reusable learning:** Validate both Core Foundation and Accessibility payload types before reading a provider-owned geometry buffer.
- **References:** `scripts/CodexWindowAXValueDecoder.swift`, `scripts/codex-window-restore.swift`, `tests/CodexWindowRestoreAXValueTests.swift`, `scripts/test_swift.sh`, `scripts/install.sh`.
