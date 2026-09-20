#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SCRIPT="${PROJECT_DIR}/scripts/install.sh"
PROCESS_GUARD="${PROJECT_DIR}/scripts/install_monitor_process_guard.sh"

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
/usr/bin/grep -F 'wait_for_restart_worker() {' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer does not preserve and wait for an in-flight restart worker'
/usr/bin/grep -F 'retire_launchd_job "com.codex.switcher.restart-worker"' "${INSTALL_SCRIPT}" >/dev/null &&
    fail 'installer force-retires an in-flight restart worker'
/usr/bin/grep -F 'monitor_process_identity() {' "${PROCESS_GUARD}" >/dev/null ||
    fail 'installer does not verify exact Monitor executable identities'
/usr/bin/grep -F '/bin/kill -TERM "${pid}"' "${PROCESS_GUARD}" >/dev/null ||
    fail 'installer does not terminate the verified Monitor PID'
/usr/bin/grep -F 'CodexMonitor PID identity changed' "${PROCESS_GUARD}" >/dev/null ||
    fail 'installer does not reject a reused Monitor PID before signalling'
/usr/bin/grep -F 'WRITERS_QUIESCED=1' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer cannot restore availability after a post-quiescence failure'
/usr/bin/grep -F 'pids="$(monitor_process_ids)" || return 1' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer treats Monitor process-enumeration errors as absence'
/usr/bin/grep -F 'while [ -n "${pid}" ] && /bin/kill -0 "${pid}"' "${INSTALL_SCRIPT}" >/dev/null ||
    fail 'installer does not wait for the captured daemon PID to exit'

sign_line="$(exact_line 'codesign --verify --deep --strict "${APP_STAGING_PATH}"')"
stop_line="$(exact_line 'stop_monitor_log_writers')"
redact_line="$(exact_line 'ensure_private_monitor_logs')"
stage_line="$(exact_line 'prepare_app_bundle_staging "${APP_DIR}" "${INSTALL_DIR}" "${BUNDLE_NAME}"')"
activate_line="$(exact_line 'activate_app_bundle_staging "${INSTALL_DIR}/${BUNDLE_NAME}"')"
writer_flag_line="$(exact_line '    WRITERS_QUIESCED=1')"
daemon_retire_line="$(exact_line '    retire_launchd_job \')"
terminate_line="$(exact_line '        terminate_verified_monitor_process \')"
worker_wait_line="$(exact_line '    wait_for_restart_worker')"

[ "${stage_line}" -lt "${stop_line}" ] || fail 'bundle staging occurs after writers are stopped'
[ "${sign_line}" -lt "${stop_line}" ] || fail 'writers are stopped before fallible app build/signing'
[ "${stop_line}" -lt "${redact_line}" ] || fail 'redaction starts before writers are quiesced'
[ "${redact_line}" -lt "${activate_line}" ] || fail 'installed app is replaced before redaction succeeds'
[ "${writer_flag_line}" -lt "${daemon_retire_line}" ] ||
    fail 'rollback is not armed before writer state changes'
[ "${daemon_retire_line}" -lt "${terminate_line}" ] ||
    fail 'daemon is not retired before the verified Monitor process'
[ "${terminate_line}" -lt "${worker_wait_line}" ] ||
    fail 'restart worker is checked before its producers are stopped'

printf 'installer log-redaction quiescence order: ok\n'
