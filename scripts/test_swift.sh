#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "🚀 Building and running Swift Test Suites for OpenAI Usage Monitor..."

TMP_BIN_DIR=$(mktemp -d /tmp/openai_swift_tests_XXXXXX)
trap 'rm -rf "${TMP_BIN_DIR}"' EXIT

cd "${REPO_DIR}"

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
    Sources/AutoLaunchManager.swift \
    Sources/SingleInstanceGuard.swift \
    Sources/AppDelegate.swift \
    tests/AppDelegateTests.swift \
    -o "${TMP_BIN_DIR}/app_delegate_test"
"${TMP_BIN_DIR}/app_delegate_test"

echo ""
echo "🎉 ALL SWIFT TEST SUITES PASSED CLEANLY!"
