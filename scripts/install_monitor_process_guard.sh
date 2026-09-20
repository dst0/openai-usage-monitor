#!/bin/bash

# Process identity checks are intentionally kept separate from the installation
# flow so a PID cannot be trusted across the daemon-retirement wait.

monitor_process_snapshot() {
    local pid="$1"
    local executable started_at
    executable="$(/bin/ps -p "${pid}" -o comm= 2>/dev/null | /usr/bin/sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')" || return 1
    started_at="$(LC_ALL=C /bin/ps -p "${pid}" -o lstart= 2>/dev/null | /usr/bin/sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')" || return 1
    [ -n "${executable}" ] && [ -n "${started_at}" ] || return 1
    /usr/bin/printf '%s|%s\n' "${executable}" "${started_at}"
}

monitor_process_identity() {
    local pid="$1"
    local snapshot executable started_at
    snapshot="$(monitor_process_snapshot "${pid}")" || return 1
    executable="${snapshot%%|*}"
    started_at="${snapshot#*|}"
    case "${executable}" in
        "/Applications/${BUNDLE_NAME}/Contents/MacOS/CodexMonitor"|\
        "${HOME}/Applications/${BUNDLE_NAME}/Contents/MacOS/CodexMonitor"|\
        "${INSTALL_DIR}/${BUNDLE_NAME}/Contents/MacOS/CodexMonitor")
            /usr/bin/printf '%s|%s\n' "${executable}" "${started_at}"
            ;;
        *) return 1 ;;
    esac
}

terminate_verified_monitor_process() {
    local pid="$1"
    local expected_identity="$2"
    local current_identity

    # A gone process is benign. A process that still exists but cannot be
    # inspected or no longer matches the captured executable/start time is a
    # fail-closed error and is never signalled.
    if ! /bin/kill -0 "${pid}" 2>/dev/null; then
        if monitor_process_snapshot "${pid}" >/dev/null 2>&1; then
            echo "❌ Refusing log migration: cannot signal a live Monitor PID." >&2
            return 1
        fi
        return 0
    fi
    if ! current_identity="$(monitor_process_identity "${pid}")"; then
        if ! /bin/kill -0 "${pid}" 2>/dev/null; then
            return 0
        fi
        echo "❌ Refusing log migration: CodexMonitor PID has an unexpected executable." >&2
        return 1
    fi
    if [ "${current_identity}" != "${expected_identity}" ]; then
        echo "❌ Refusing log migration: CodexMonitor PID identity changed." >&2
        return 1
    fi
    /bin/kill -TERM "${pid}"
}
