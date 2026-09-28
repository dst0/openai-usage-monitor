#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=swift_module_cache.sh
source "${SCRIPT_DIR}/swift_module_cache.sh"

echo "🚀 Building and running Swift Test Suites for OpenAI Usage Monitor..."
# Resolve an overridden module cache before any swiftc call; see
# scripts/swift_module_cache.sh.
canonicalize_clang_module_cache_path || exit 1

TMP_BIN_DIR=$(mktemp -d /tmp/openai_swift_tests_XXXXXX)
trap 'rm -rf "${TMP_BIN_DIR}"' EXIT

cd "${REPO_DIR}"

echo "👉 Running Swift module-cache path tests..."
bash tests/swift_module_cache_path.sh

echo "👉 Checking that Swift tests keep their own defaults store..."
bash tests/swift_test_defaults_isolation.sh

echo "👉 [1/2] Running Screen Contrast, Vector Icons & Stacked Percentage Tests..."
swiftc -parse-as-library \
    Sources/StatusBarStyle.swift \
    tests/ScreenContrastTests.swift \
    -o "${TMP_BIN_DIR}/screen_contrast_test"
"${TMP_BIN_DIR}/screen_contrast_test"

echo "👉 [2/2] Running AppDelegate Status Bar & Layout Tests..."
swiftc -parse-as-library \
    Sources/Localization.swift \
    Sources/QuotaModels.swift \
    Sources/StatusBarStyle.swift \
    Sources/CodexClient.swift \
    Sources/CodexDesktopProcessIdentity.swift \
    Sources/CodexRecoveryProcessIdentity.swift \
    Sources/AutoLaunchManager.swift \
    Sources/LaunchAtLoginMenuController.swift \
    Sources/SingleInstanceGuard.swift \
    Sources/MenuIconButton.swift \
    Sources/InsetSeparatorView.swift \
    Sources/PrimaryMenuSectionHeaderView.swift \
    Sources/AccountSectionHeaderView.swift \
    Sources/ReserveAccountSectionEntry.swift \
    Sources/ResetCreditsRowView.swift \
    Sources/AccountRowView.swift \
    Sources/AccountSwitchButtonsView.swift \
    Sources/AccountSectionCardView.swift \
    Sources/AccountSectionCardView+Tracking.swift \
    Sources/AppDelegate.swift \
    Sources/AppDelegate+FileWatchers.swift \
    Sources/AppDelegate+DesktopLifecycle.swift \
    Sources/StatusBarBracketRenderer.swift \
    Sources/AppDelegate+StatusBar.swift \
    Sources/AppDelegate+StatusBarOverloads.swift \
    Sources/AppDelegate+Menu.swift \
    Sources/AppDelegate+AppBlock.swift \
    Sources/AppDelegate+CliBlock.swift \
    Sources/AppDelegate+ReserveCards.swift \
    Sources/AppDelegate+DynamicItems.swift \
    Sources/AppDelegate+AccountActions.swift \
    Sources/AppDelegate+AutoSwitch.swift \
    Sources/AppDelegate+AutoReset.swift \
    Sources/AppDelegate+SettingsActions.swift \
    Sources/AppDelegate+WindowBounds.swift \
    tests/TestPreferencesSuite.swift \
    tests/AppDelegatePreferencesTests.swift \
    tests/FakeLoginItems.swift \
    tests/LaunchAtLoginTests.swift \
    tests/AppDelegateTests.swift \
    -o "${TMP_BIN_DIR}/app_delegate_test"
"${TMP_BIN_DIR}/app_delegate_test"

echo "👉 Running test defaults suite guard and cleanup tests..."
swiftc -parse-as-library \
    tests/TestPreferencesSuite.swift \
    tests/TestPreferencesSuiteTests.swift \
    -o "${TMP_BIN_DIR}/test-preferences-suite_test"
"${TMP_BIN_DIR}/test-preferences-suite_test"

echo "👉 Running recovery payload security tests..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    Sources/CodexRecoveryPayloadReader.swift \
    tests/CodexRecoveryPayloadReaderTests.swift \
    -o "${TMP_BIN_DIR}/codex-recovery-payload-reader_test"
"${TMP_BIN_DIR}/codex-recovery-payload-reader_test"

echo "👉 Running recovery banner geometry tests..."
RECOVERY_BANNER_SOURCES=()
while IFS= read -r recovery_source || [ -n "${recovery_source}" ]; do
    [ -n "${recovery_source}" ] || continue
    RECOVERY_BANNER_SOURCES+=("${recovery_source}")
done < "scripts/codex-recovery-banner-sources.txt"
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    "${RECOVERY_BANNER_SOURCES[@]}" \
    tests/CodexRecoveryBannerGeometryTests.swift \
    -o "${TMP_BIN_DIR}/codex-recovery-banner-geometry_test"
"${TMP_BIN_DIR}/codex-recovery-banner-geometry_test"

echo "👉 Running App/CLI identity separation tests..."
swiftc -parse-as-library \
    Sources/Localization.swift \
    Sources/QuotaModels.swift \
    Sources/CodexClient.swift \
    Sources/CodexDesktopProcessIdentity.swift \
    Sources/CodexRecoveryProcessIdentity.swift \
    tests/CodexClientIdentityTests.swift \
    -o "${TMP_BIN_DIR}/codex-client-identity_test"
"${TMP_BIN_DIR}/codex-client-identity_test"

echo "👉 Compiling recovery banner and exact window helpers..."
RECOVERY_BANNER_SOURCES=()
while IFS= read -r recovery_source || [ -n "${recovery_source}" ]; do
    [ -n "${recovery_source}" ] || continue
    RECOVERY_BANNER_SOURCES+=("${recovery_source}")
done < "scripts/codex-recovery-banner-sources.txt"
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    "${RECOVERY_BANNER_SOURCES[@]}" \
    scripts/codex-recovery-banner-main.swift \
    -o "${TMP_BIN_DIR}/codex-recovery-banner"
swiftc \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    scripts/CodexWindowAXValueDecoder.swift \
    scripts/CodexWindowSafetyChecks.swift \
    scripts/CodexWindowTaskProbeValidation.swift \
    scripts/CodexWindowTaskProbeKeyboard.swift \
    scripts/CodexWindowTaskProbeCore.swift \
    scripts/CodexWindowTaskSessionSystem.swift \
    scripts/CodexWindowTaskSessionCore.swift \
    scripts/CodexPreservedClipboard.swift \
    scripts/CodexWindowTaskRecords.swift \
    scripts/CodexWindowTaskProbe.swift \
    scripts/CodexWindowTaskSession.swift \
    scripts/codex-window-restore.swift \
    -o "${TMP_BIN_DIR}/codex-window-restore"

echo "👉 Running exact window Accessibility value tests..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    scripts/CodexWindowAXValueDecoder.swift \
    tests/CodexWindowRestoreAXValueTests.swift \
    -o "${TMP_BIN_DIR}/codex-window-ax-value_test"
"${TMP_BIN_DIR}/codex-window-ax-value_test"

echo "👉 Running exact window safety checks..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    scripts/CodexWindowSafetyChecks.swift \
    tests/CodexWindowSafetyChecksTests.swift \
    -o "${TMP_BIN_DIR}/codex-window-safety-checks_test"
"${TMP_BIN_DIR}/codex-window-safety-checks_test"

echo "👉 Running selected-task probe validation tests..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    scripts/CodexWindowSafetyChecks.swift \
    scripts/CodexWindowTaskProbeValidation.swift \
    scripts/CodexWindowTaskProbeKeyboard.swift \
    tests/CodexWindowTaskProbeTests.swift \
    -o "${TMP_BIN_DIR}/codex-window-task-probe_test"
"${TMP_BIN_DIR}/codex-window-task-probe_test"

echo "👉 Running selected-task probe sequencing tests..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework Foundation \
    scripts/CodexWindowSafetyChecks.swift \
    scripts/CodexWindowTaskProbeValidation.swift \
    scripts/CodexWindowTaskProbeCore.swift \
    tests/CodexWindowTaskProbeCoreTests.swift \
    -o "${TMP_BIN_DIR}/codex-window-task-probe-core_test"
"${TMP_BIN_DIR}/codex-window-task-probe-core_test"

echo "👉 Running window task snapshot, restore, and rehearsal sequencing tests..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    scripts/CodexWindowSafetyChecks.swift \
    scripts/CodexWindowTaskProbeValidation.swift \
    scripts/CodexWindowTaskProbeCore.swift \
    scripts/CodexWindowTaskSessionSystem.swift \
    scripts/CodexWindowTaskSessionCore.swift \
    tests/CodexWindowTaskFakeDesktop.swift \
    tests/CodexWindowTaskSessionCoreTests.swift \
    -o "${TMP_BIN_DIR}/codex-window-task-session_test"
"${TMP_BIN_DIR}/codex-window-task-session_test"

echo "👉 Running window task record contract tests..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework Foundation \
    scripts/CodexWindowSafetyChecks.swift \
    scripts/CodexWindowTaskProbeValidation.swift \
    scripts/CodexWindowTaskProbeCore.swift \
    scripts/CodexWindowTaskSessionSystem.swift \
    scripts/CodexWindowTaskRecords.swift \
    tests/CodexWindowTaskRecordsTests.swift \
    -o "${TMP_BIN_DIR}/codex-window-task-records_test"
"${TMP_BIN_DIR}/codex-window-task-records_test"

echo "👉 Running preserved clipboard tests on a private pasteboard..."
swiftc -parse-as-library \
    -target "$(uname -m)-apple-macosx13.0" \
    -framework AppKit -framework Foundation -framework ApplicationServices \
    scripts/CodexWindowSafetyChecks.swift \
    scripts/CodexWindowTaskProbeValidation.swift \
    scripts/CodexPreservedClipboard.swift \
    tests/CodexPreservedClipboardTests.swift \
    -o "${TMP_BIN_DIR}/codex-preserved-clipboard_test"
"${TMP_BIN_DIR}/codex-preserved-clipboard_test"

echo ""
echo "🎉 ALL SWIFT TEST SUITES PASSED CLEANLY!"
