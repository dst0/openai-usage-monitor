#!/bin/bash
# Prints what the Monitor's live lookups find on this Mac: the installed Monitor
# CLI (CodexClient.installedCLIExecutable) and the running official Desktop
# (CodexDesktopProcessIdentity.current, which reads the process list). The
# Swift tests never make these lookups; they use fakes. Run this only when you
# need that live evidence, for example to see which codex-mon the menu would
# run:
#
#   scripts/swift_live_diagnostics.sh --allow-live-system
#
# It only reads the live system: it runs no CLI, reads no Codex home, and
# writes only its temporary build directory and the Swift module cache. It
# builds there, never beside resources/Info.plist.
#
# scripts/test_swift.sh passes --compile-only, which builds the same program
# and runs nothing, so its source list cannot go stale. Any other arguments
# stop here, before the build. No test or workflow passes --allow-live-system
# (tests/swift_test_live_system_isolation.sh checks that, and checks both
# modes with a fake swiftc).
set -euo pipefail

if [ "$#" -eq 1 ] && [ "$1" = "--allow-live-system" ]; then
    RUN_DIAGNOSTIC=1
elif [ "$#" -eq 1 ] && [ "$1" = "--compile-only" ]; then
    RUN_DIAGNOSTIC=0
else
    echo "usage: scripts/swift_live_diagnostics.sh --allow-live-system | --compile-only" >&2
    echo "--allow-live-system reads the live process list and looks for the installed Monitor CLI; pass it only when you need that." >&2
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
    Sources/BoundedCommand.swift \
    Sources/CodexClient.swift \
    Sources/CodexDesktopProcessIdentity.swift \
    Sources/CodexRecoveryProcessIdentity.swift \
    scripts/codex-live-diagnostics.swift \
    -o "${TMP_BIN_DIR}/codex-live-diagnostics"
[ "${RUN_DIAGNOSTIC}" -eq 1 ] || exit 0
"${TMP_BIN_DIR}/codex-live-diagnostics"
