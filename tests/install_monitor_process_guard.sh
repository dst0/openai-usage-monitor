#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-monitor-process-guard.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT INT TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

FAKE_BIN="${TEMP_ROOT}/bin"
/bin/mkdir -p "${FAKE_BIN}"
FAKE_STATE="${TEMP_ROOT}/state"
KILL_LOG="${TEMP_ROOT}/kill.log"

cat > "${FAKE_BIN}/ps" <<'EOF'
#!/bin/bash
if [ "$(/bin/cat "${FAKE_STATE}.alive")" != 0 ]; then
    exit 1
fi
case " $* " in
    *' comm= '*) /bin/cat "${FAKE_STATE}.executable" ;;
    *' lstart= '*) /bin/cat "${FAKE_STATE}.started" ;;
    *) exit 2 ;;
esac
EOF
cat > "${FAKE_BIN}/kill" <<'EOF'
#!/bin/bash
if [ "$1" = "-0" ]; then
    exit "$(/bin/cat "${FAKE_STATE}.alive")"
fi
/usr/bin/printf '%s\n' "$*" >> "${KILL_LOG}"
EOF
/bin/chmod 755 "${FAKE_BIN}/ps" "${FAKE_BIN}/kill"

GUARD_COPY="${TEMP_ROOT}/install_monitor_process_guard.sh"
/usr/bin/sed \
    -e "s|/bin/ps|${FAKE_BIN}/ps|g" \
    -e "s|/bin/kill|${FAKE_BIN}/kill|g" \
    "${PROJECT_DIR}/scripts/install_monitor_process_guard.sh" > "${GUARD_COPY}"

BUNDLE_NAME="Codex Monitor.app"
INSTALL_DIR="${TEMP_ROOT}/Applications"
LOCAL_BIN="${TEMP_ROOT}/local-bin"
HOME="${TEMP_ROOT}/home"
export FAKE_STATE KILL_LOG
source "${GUARD_COPY}"

/usr/bin/printf '%s\n' "${INSTALL_DIR}/${BUNDLE_NAME}/Contents/MacOS/CodexMonitor" > "${FAKE_STATE}.executable"
/usr/bin/printf '%s\n' 'Sun Sep 20 12:00:00 2026' > "${FAKE_STATE}.started"
/usr/bin/printf '0\n' > "${FAKE_STATE}.alive"
SNAPSHOT="$(monitor_process_identity 4242)"

/usr/bin/printf '%s\n' 'Sun Sep 20 12:00:01 2026' > "${FAKE_STATE}.started"
if terminate_verified_monitor_process 4242 "${SNAPSHOT}" 2>/dev/null; then
    fail 'changed process start time was signalled'
fi
[ ! -e "${KILL_LOG}" ] || fail 'changed PID identity reached kill'

/usr/bin/printf '%s\n' 'Sun Sep 20 12:00:00 2026' > "${FAKE_STATE}.started"
/usr/bin/printf '1\n' > "${FAKE_STATE}.alive"
terminate_verified_monitor_process 4242 "${SNAPSHOT}"
[ ! -e "${KILL_LOG}" ] || fail 'gone PID reached kill'

/usr/bin/printf '0\n' > "${FAKE_STATE}.alive"
terminate_verified_monitor_process 4242 "${SNAPSHOT}"
[ "$(/bin/cat "${KILL_LOG}")" = '-TERM 4242' ] || fail 'unchanged PID was not signalled exactly once'

printf 'installer Monitor PID identity guard: ok\n'
