#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-worker-wait-test.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT INT TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# The fakes' paths are spliced into sed programs below, which only works for
# plain path characters.
case "${TEMP_ROOT}" in
    *[!A-Za-z0-9/._+-]*) fail "TMPDIR must be a path of letters, digits, /, ., _, +, and -: ${TEMP_ROOT}" ;;
esac

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

# The copies may run launchctl and sleep only through the fakes. A bare
# `launchctl` or another spelling the rewrite missed would query, or after a
# regression remove, the real restart worker, and a sourced file other than
# the rewritten helper copy would run unrewritten, so fail before running them.
SYSTEM_COMMAND='(^|[^[:alnum:]_.])(launchctl|sleep|kill|pkill|killall|ps|pgrep|lsappinfo|osascript|open)([^[:alnum:]_.-]|$)'
SOURCE_COMMAND='(^|[;&|({[:space:]])(source|[.])[[:space:]]'
# The wait script's only source line, which resolves next to the copy.
HELPER_SOURCE='^[0-9]+:source "[$][{]SCRIPT_DIR[}]/install_launchd_helpers[.]sh"$'
# unfaked_commands FILE: the non-comment lines of FILE that still name a
# system command once the fakes' paths are removed, or that source a file
# other than the helper copy.
unfaked_commands() {
    /usr/bin/sed -e "s|${FAKE_LAUNCHCTL}||g" -e "s|${FAKE_SLEEP}||g" "$1" |
        /usr/bin/grep -n -E "${SYSTEM_COMMAND}|${SOURCE_COMMAND}" |
        /usr/bin/grep -v -E -e '^[0-9]+:[[:space:]]*#' -e "${HELPER_SOURCE}" || true
}
assert_only_fakes() {
    local copy unfaked
    for copy in "$@"; do
        unfaked="$(unfaked_commands "${copy}")"
        [ -z "${unfaked}" ] || fail "${copy##*/} can still reach a real system command:
${unfaked}"
    done
}
# The check must reject each of these lines, and accept the fakes, comments,
# and the helper source line.
PROBE="${TEMP_ROOT}/guard-probe.sh"
for line in 'launchctl list x' '"${LAUNCHCTL:-launchctl}" list' '/bin/launchctl remove x' \
    '/sbin/launchctl bootout gui/501/x' '    sleep 0.1' '/usr/bin/pkill -f worker' \
    '/usr/bin/osascript -e quit' 'source "${PROJECT_DIR}/scripts/install_launchd_helpers.sh"' \
    '. "${SCRIPT_DIR}/other.sh"'; do
    /usr/bin/printf '%s\n' "${line}" > "${PROBE}"
    [ -n "$(unfaked_commands "${PROBE}")" ] || fail "fake guard missed: ${line}"
done
/usr/bin/printf '%s\n' "${FAKE_LAUNCHCTL} list x" "    ${FAKE_SLEEP} 0.1" '# launchctl sleep' \
    'source "${SCRIPT_DIR}/install_launchd_helpers.sh"' 'LABEL="com.codex.switcher.restart-worker"' \
    > "${PROBE}"
[ -z "$(unfaked_commands "${PROBE}")" ] || fail "fake guard rejected fakes, comments, or the helper source"

/usr/bin/sed \
    -e "s|/bin/launchctl|${FAKE_LAUNCHCTL}|g" \
    "${PROJECT_DIR}/scripts/install_launchd_helpers.sh" > "${HELPER_COPY}"
/usr/bin/sed -e "s|/bin/sleep|${FAKE_SLEEP}|g" \
    "${PROJECT_DIR}/scripts/wait_for_restart_worker.sh" > "${SCRIPT_COPY}"
/bin/chmod 755 "${SCRIPT_COPY}"
assert_only_fakes "${HELPER_COPY}" "${SCRIPT_COPY}"
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
assert_only_fakes "${SCRIPT_COPY}"
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
