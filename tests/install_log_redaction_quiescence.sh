#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SCRIPT="${PROJECT_DIR}/scripts/install.sh"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

exact_line() {
    local text="$1"
    /usr/bin/grep -n -F -x "$text" "${INSTALL_SCRIPT}" | /usr/bin/head -n 1 | /usr/bin/cut -d: -f1
}

/usr/bin/grep -F 'stop_monitor_log_writers() {' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer has no writer-quiescence boundary'
/usr/bin/grep -F 'com.codex.switcher.restart-worker' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer does not retire the restart worker'
/usr/bin/grep -F '/bin/ps -p "${pid}" -o comm=' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer does not verify exact Monitor executable identities'
/usr/bin/grep -F '/bin/kill -TERM "${pid}"' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer does not terminate the verified Monitor PID'
/usr/bin/grep -F 'WRITERS_QUIESCED=1' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer cannot restore availability after a post-quiescence failure'

sign_line="$(exact_line 'codesign --force --deep --sign - "${APP_DIR}" 2>/dev/null || true')"
stop_line="$(exact_line 'stop_monitor_log_writers')"
redact_line="$(exact_line 'ensure_private_monitor_logs')"
replace_line="$(exact_line 'rm -rf "${INSTALL_DIR}/${BUNDLE_NAME}"')"
writer_flag_line="$(exact_line '    WRITERS_QUIESCED=1')"
restart_retire_line="$(exact_line '    retire_launchd_job "com.codex.switcher.restart-worker"')"
kill_line="$(exact_line '        /bin/kill -TERM "${pid}"')"

[ "${sign_line}" -lt "${stop_line}" ] || fail 'writers are stopped before fallible app build/signing'
[ "${stop_line}" -lt "${redact_line}" ] || fail 'redaction starts before writers are quiesced'
[ "${redact_line}" -lt "${replace_line}" ] || fail 'installed app is replaced before redaction succeeds'
[ "${writer_flag_line}" -lt "${restart_retire_line}" ] ||
    fail 'rollback is not armed before writer state changes'
[ "${restart_retire_line}" -lt "${kill_line}" ] ||
    fail 'restart worker is not retired before the verified Monitor process'

printf 'installer log-redaction quiescence order: ok\n'
