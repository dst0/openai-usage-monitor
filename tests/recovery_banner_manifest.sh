#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${PROJECT_DIR}/scripts/codex-recovery-banner-sources.txt"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

[ -f "${MANIFEST}" ] || fail "missing recovery banner source manifest"
grep -Fq 'codex-recovery-banner-sources.txt' "${PROJECT_DIR}/scripts/install.sh" \
    || fail "installer does not consume recovery banner source manifest"
grep -Fq 'codex-recovery-banner-sources.txt' "${PROJECT_DIR}/scripts/test_swift.sh" \
    || fail "Swift test helper does not consume recovery banner source manifest"

sources=()
while IFS= read -r source || [ -n "${source}" ]; do
    [ -n "${source}" ] || continue
    [ -f "${PROJECT_DIR}/${source}" ] || fail "manifest source is missing: ${source}"
    sources+=("${PROJECT_DIR}/${source}")
done < "${MANIFEST}"

[ "${#sources[@]}" -gt 0 ] || fail "recovery banner source manifest is empty"

if [ "$(uname -s)" = "Darwin" ] && command -v swiftc >/dev/null 2>&1; then
    tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/codex-recovery-banner-manifest.XXXXXX")"
    cleanup() { rm -rf "${tmp_dir}"; }
    trap cleanup EXIT INT TERM
    swiftc -parse-as-library \
        -target "$(uname -m)-apple-macosx13.0" \
        -framework AppKit -framework Foundation -framework ApplicationServices \
        "${sources[@]}" \
        "${PROJECT_DIR}/scripts/codex-recovery-banner-main.swift" \
        -o "${tmp_dir}/codex-recovery-banner"
fi

echo "recovery banner source manifest: ok"
