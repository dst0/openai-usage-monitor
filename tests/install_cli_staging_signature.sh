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
cleanup_guard_line="$(line_number '    if [ -n "${CLI_STAGING}" ]; then')"
cleanup_remove_line="$(line_number '        rm -f "${CLI_STAGING}" "${CLI_STAGING}.cstemp"')"
cleanup_trap_line="$(line_number 'trap cleanup EXIT')"
signal_int_trap_line="$(line_number "trap 'exit 130' INT")"
signal_term_trap_line="$(line_number "trap 'exit 143' TERM")"
signal_quit_trap_line="$(line_number "trap 'exit 131' QUIT")"
mktemp_line="$(line_number 'CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"')"
fingerprint_guard_line="$(line_number '    if [[ ! "${CODEX_MONITOR_SIGNING_IDENTITY_SHA1}" =~ ^[0-9A-Fa-f]{40}$ ]]; then')"
identity_lookup_line="$(line_number '    if ! /usr/bin/security find-identity -v -p codesigning 2>/dev/null |')"
identity_selection_line="$(line_number '    MONITOR_SIGNING_IDENTITY="${CODEX_MONITOR_SIGNING_IDENTITY_SHA1}"')"
copy_line="$(line_number 'cp "target/release/codex-mon" "${CLI_STAGING}"')"
chmod_line="$(line_number 'chmod 755 "${CLI_STAGING}"')"
xattr_line="$(line_number 'xattr -c "${CLI_STAGING}" 2>/dev/null || true')"
sign_line="$(line_number 'codesign --sign "${MONITOR_SIGNING_IDENTITY}" --identifier com.codex.monitor.cli --force "${CLI_STAGING}"')"
verify_line="$(line_number 'codesign --verify --strict "${CLI_STAGING}"')"
smoke_line="$(line_number '"${CLI_STAGING}" --version >/dev/null')"
move_line="$(line_number 'mv -f "${CLI_STAGING}" "${LOCAL_BIN}/codex-mon"')"
clear_line="$(line_number_after 'CLI_STAGING=""' "${move_line}")"
helper_cleanup_guard_line="$(line_number '    if [ -n "${WINDOW_HELPER_STAGING}" ]; then')"
helper_cleanup_remove_line="$(line_number '        rm -f "${WINDOW_HELPER_STAGING}" "${WINDOW_HELPER_STAGING}.cstemp"')"
helper_mktemp_line="$(line_number '    WINDOW_HELPER_STAGING="$(mktemp "${LOCAL_BIN}/.codex-window-restore.install.XXXXXX")"')"
helper_compile_output_line="$(line_number '        -o "${WINDOW_HELPER_STAGING}" \')"
helper_chmod_line="$(line_number '    chmod 755 "${WINDOW_HELPER_STAGING}"')"
helper_xattr_line="$(line_number '    xattr -c "${WINDOW_HELPER_STAGING}" 2>/dev/null || true')"
helper_sign_line="$(line_number '    codesign --sign "${MONITOR_SIGNING_IDENTITY}" --identifier com.codex.monitor.window-restore --force "${WINDOW_HELPER_STAGING}"')"
helper_verify_line="$(line_number '    codesign --verify --strict "${WINDOW_HELPER_STAGING}"')"
helper_lock_recheck_line="$(line_number_after '    install_lock_still_named || exit 1' "${helper_verify_line}")"
helper_move_line="$(line_number '    mv -f "${WINDOW_HELPER_STAGING}" "${LOCAL_BIN}/codex-window-restore"')"
helper_clear_line="$(line_number_after '    WINDOW_HELPER_STAGING=""' "${helper_move_line}")"

if ! {
    [ "${fingerprint_guard_line}" -lt "${identity_lookup_line}" ] &&
        [ "${identity_lookup_line}" -lt "${identity_selection_line}" ] &&
        [ "${identity_selection_line}" -lt "${mktemp_line}" ] &&
    [ "${cleanup_function_line}" -lt "${cleanup_guard_line}" ] &&
        [ "${cleanup_guard_line}" -lt "${cleanup_remove_line}" ] &&
        [ "${cleanup_remove_line}" -lt "${cleanup_trap_line}" ] &&
        [ "${cleanup_trap_line}" -lt "${signal_int_trap_line}" ] &&
        [ "${signal_int_trap_line}" -lt "${signal_term_trap_line}" ] &&
        [ "${signal_term_trap_line}" -lt "${signal_quit_trap_line}" ] &&
        [ "${signal_quit_trap_line}" -lt "${mktemp_line}" ]
}; then
    echo "CLI staging failure cleanup must remove the staged file through EXIT cleanup with signal exits" >&2
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
        echo "CLI staging commands must remain contiguous and ordered: fresh inode, copy, chmod, xattr cleanup, selected-identity sign, strict verify, launch smoke, atomic move, cleanup reset" >&2
        exit 1
    fi
done

if ! {
    [ "${cleanup_function_line}" -lt "${helper_cleanup_guard_line}" ] &&
        [ "${helper_cleanup_guard_line}" -lt "${helper_cleanup_remove_line}" ] &&
        [ "${helper_cleanup_remove_line}" -lt "${cleanup_trap_line}" ] &&
        [ "${identity_selection_line}" -lt "${helper_mktemp_line}" ] &&
        [ "${helper_mktemp_line}" -lt "${helper_compile_output_line}" ] &&
        [ "${helper_compile_output_line}" -lt "${helper_chmod_line}" ] &&
        [ "${helper_chmod_line}" -lt "${helper_xattr_line}" ] &&
        [ "${helper_xattr_line}" -lt "${helper_sign_line}" ] &&
        [ "${helper_sign_line}" -lt "${helper_verify_line}" ] &&
        [ "${helper_verify_line}" -lt "${helper_lock_recheck_line}" ] &&
        [ "${helper_lock_recheck_line}" -lt "${helper_move_line}" ] &&
        [ "${helper_move_line}" -lt "${helper_clear_line}" ]
}; then
    echo "window helper must compile to staging, sign and verify with the selected identity, then atomically replace its path" >&2
    exit 1
fi

echo "installer CLI and window helper staging signature order: ok"
