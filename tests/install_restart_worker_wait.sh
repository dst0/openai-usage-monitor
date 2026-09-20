#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-worker-wait-test.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT INT TERM

COUNTER="${TEMP_ROOT}/counter"
REMOVED="${TEMP_ROOT}/removed"
MODE="${TEMP_ROOT}/mode"
FAKE_LAUNCHCTL="${TEMP_ROOT}/launchctl"
FAKE_SLEEP="${TEMP_ROOT}/sleep"
SCRIPT_COPY="${TEMP_ROOT}/wait.sh"
HELPER_COPY="${TEMP_ROOT}/install_launchd_helpers.sh"

/usr/bin/printf '0\n' > "${COUNTER}"
/usr/bin/printf 'normal\n' > "${MODE}"
/bin/cat > "${FAKE_LAUNCHCTL}" <<EOF
#!/bin/bash
if [ "\$(/bin/cat "${MODE}")" = error ]; then
    exit 2
fi
if [ "\$(/bin/cat "${MODE}")" = fallback ]; then
    if [ "\$#" -eq 1 ]; then
        /usr/bin/printf '%b\n' '-\t0\tcom.codex.switcher.restart-worker'
        exit 0
    fi
    exit 2
fi
if [ "\${1:-}" != list ]; then
    /usr/bin/touch "${REMOVED}"
    exit 2
fi
if [ "\$#" -eq 1 ]; then
    /usr/bin/printf 'PID\tStatus\tLabel\n'
    exit 0
fi
if [ "\${2:-}" != com.codex.switcher.restart-worker ]; then
    /usr/bin/touch "${REMOVED}"
    exit 2
fi
count="\$(/bin/cat "${COUNTER}")"
count=\$((count + 1))
/usr/bin/printf '%s\n' "\${count}" > "${COUNTER}"
[ "\${count}" -le 2 ]
EOF
/usr/bin/printf '#!/bin/bash\nexit 0\n' > "${FAKE_SLEEP}"
/bin/chmod 755 "${FAKE_LAUNCHCTL}" "${FAKE_SLEEP}"

/usr/bin/sed \
    -e "s|/bin/launchctl|${FAKE_LAUNCHCTL}|g" \
    "${PROJECT_DIR}/scripts/install_launchd_helpers.sh" > "${HELPER_COPY}"
/usr/bin/sed -e "s|/bin/sleep|${FAKE_SLEEP}|g" \
    "${PROJECT_DIR}/scripts/wait_for_restart_worker.sh" > "${SCRIPT_COPY}"
/bin/chmod 755 "${SCRIPT_COPY}"
"${SCRIPT_COPY}"
[ "$(/bin/cat "${COUNTER}")" = 3 ] || { echo 'FAIL: worker was not polled to completion' >&2; exit 1; }
[ ! -e "${REMOVED}" ] || { echo 'FAIL: worker wait attempted a destructive command' >&2; exit 1; }

# Prove the wait also fails closed rather than removing a stuck worker. Lower
# only the copied test bound; production remains ten minutes.
/usr/bin/printf '0\n' > "${COUNTER}"
/usr/bin/sed \
    -e "s|/bin/sleep|${FAKE_SLEEP}|g" \
    -e 's/-ge 6000/-ge 2/' \
    "${PROJECT_DIR}/scripts/wait_for_restart_worker.sh" > "${SCRIPT_COPY}"
/bin/chmod 755 "${SCRIPT_COPY}"
if "${SCRIPT_COPY}" >/dev/null; then
    echo 'FAIL: stuck worker was accepted' >&2
    exit 1
fi
[ ! -e "${REMOVED}" ] || { echo 'FAIL: stuck worker was force-removed' >&2; exit 1; }

/usr/bin/printf 'error\n' > "${MODE}"
if "${SCRIPT_COPY}" >/dev/null; then
    echo 'FAIL: launchctl command error was accepted as worker absence' >&2
    exit 1
fi
[ ! -e "${REMOVED}" ] || { echo 'FAIL: command error triggered a destructive action' >&2; exit 1; }

source "${HELPER_COPY}"
/usr/bin/printf 'fallback\n' > "${MODE}"
launchd_job_present com.codex.switcher.restart-worker || {
    echo 'FAIL: full listing did not recover a failed targeted query' >&2
    exit 1
}
/usr/bin/printf 'error\n' > "${MODE}"
if launchd_job_present com.codex.switcher.restart-worker; then
    echo 'FAIL: launchctl error was reported as present' >&2
    exit 1
else
    state=$?
fi
[ "${state}" -eq 2 ] || { echo 'FAIL: launchctl error was reported as absence' >&2; exit 1; }

printf 'restart worker wait: ok\n'
