#!/bin/bash
# Prints what the Monitor's live lookups find on this Mac: the installed Monitor
# CLI (CodexClient.installedCLIExecutable) and the running official Desktop
# (CodexDesktopProcessIdentity.current, which reads the process list). The
# Swift tests never make these lookups; they use fakes. Run this only when you
# need that live evidence, for example to see which codex-mon the menu would
# run. It only reads: it runs no CLI, reads no Codex home, and changes nothing.
# It builds in a temporary directory, never beside resources/Info.plist. No test
# or CI step runs it (tests/swift_test_live_system_isolation.sh checks that).
set -euo pipefail

if [ "$#" -ne 1 ] || [ "$1" != "--allow-live-system" ]; then
    echo "usage: scripts/swift_live_diagnostics.sh --allow-live-system" >&2
    echo "It reads the live process list and looks for the installed Monitor CLI; run it only when you need that." >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=swift_module_cache.sh
source "${SCRIPT_DIR}/swift_module_cache.sh"
# Resolve an overridden module cache before the compile; see scripts/swift_module_cache.sh.
canonicalize_clang_module_cache_path || exit 1

TMP_BIN_DIR="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-live-diagnostics.XXXXXX")"
trap '/bin/rm -rf -- "${TMP_BIN_DIR}"' EXIT

cd "${REPO_DIR}"
swiftc -parse-as-library \
    Sources/Localization.swift \
    Sources/QuotaModels.swift \
    Sources/CodexClient.swift \
    Sources/CodexDesktopProcessIdentity.swift \
    Sources/CodexRecoveryProcessIdentity.swift \
    scripts/codex-live-diagnostics.swift \
    -o "${TMP_BIN_DIR}/codex-live-diagnostics"
"${TMP_BIN_DIR}/codex-live-diagnostics"
