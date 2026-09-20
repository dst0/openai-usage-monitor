#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SCRIPT="${PROJECT_DIR}/scripts/install.sh"

line_number() {
    local expected="$1"
    local matches
    matches="$(grep -n -F -x "${expected}" "${INSTALL_SCRIPT}" || true)"
    if [ "$(printf '%s\n' "${matches}" | sed '/^$/d' | wc -l | tr -d ' ')" -ne 1 ]; then
        echo "expected exactly one installer line: ${expected}" >&2
        exit 1
    fi
    printf '%s\n' "${matches%%:*}"
}

line_number_after() {
    local expected="$1"
    local after_line="$2"
    local matches
    matches="$(grep -n -F -x "${expected}" "${INSTALL_SCRIPT}" | awk -F: -v after="${after_line}" '$1 > after { print }' || true)"
    if [ "$(printf '%s\n' "${matches}" | sed '/^$/d' | wc -l | tr -d ' ')" -ne 1 ]; then
        echo "expected exactly one later installer line: ${expected}" >&2
        exit 1
    fi
    printf '%s\n' "${matches%%:*}"
}

cleanup_function_line="$(line_number 'cleanup() {')"
cleanup_guard_line="$(line_number '    if [ -n "${CLI_STAGING}" ] && [ -f "${CLI_STAGING}" ]; then')"
cleanup_remove_line="$(line_number '        rm -f "${CLI_STAGING}"')"
cleanup_trap_line="$(line_number 'trap cleanup EXIT INT TERM')"
mktemp_line="$(line_number 'CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"')"
copy_line="$(line_number 'cp "target/release/codex-mon" "${CLI_STAGING}"')"
chmod_line="$(line_number 'chmod 755 "${CLI_STAGING}"')"
xattr_line="$(line_number 'xattr -c "${CLI_STAGING}" 2>/dev/null || true')"
sign_line="$(line_number 'codesign --sign - --force "${CLI_STAGING}"')"
verify_line="$(line_number 'codesign --verify --strict "${CLI_STAGING}"')"
smoke_line="$(line_number '"${CLI_STAGING}" --version >/dev/null')"
move_line="$(line_number 'mv -f "${CLI_STAGING}" "${LOCAL_BIN}/codex-mon"')"
clear_line="$(line_number_after 'CLI_STAGING=""' "${move_line}")"

if ! {
    [ "${cleanup_function_line}" -lt "${cleanup_guard_line}" ] &&
        [ "${cleanup_guard_line}" -lt "${cleanup_remove_line}" ] &&
        [ "${cleanup_remove_line}" -lt "${cleanup_trap_line}" ] &&
        [ "${cleanup_trap_line}" -lt "${mktemp_line}" ]
}; then
    echo "CLI staging failure cleanup must remove the staged file through the EXIT/INT/TERM trap" >&2
    exit 1
fi

expected_lines=(
    "${mktemp_line}"
    "$((mktemp_line + 1))"
    "$((mktemp_line + 2))"
    "$((mktemp_line + 3))"
    "$((mktemp_line + 4))"
    "$((mktemp_line + 5))"
    "$((mktemp_line + 6))"
    "$((mktemp_line + 7))"
    "$((mktemp_line + 8))"
)
actual_lines=(
    "${mktemp_line}"
    "${copy_line}"
    "${chmod_line}"
    "${xattr_line}"
    "${sign_line}"
    "${verify_line}"
    "${smoke_line}"
    "${move_line}"
    "${clear_line}"
)

for index in "${!expected_lines[@]}"; do
    if [ "${actual_lines[${index}]}" -ne "${expected_lines[${index}]}" ]; then
        echo "CLI staging commands must remain contiguous and ordered: fresh inode, copy, chmod, xattr cleanup, unconditional ad-hoc sign, strict verify, launch smoke, atomic move, cleanup reset" >&2
        exit 1
    fi
done

echo "installer CLI staging signature order: ok"
