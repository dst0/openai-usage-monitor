#!/bin/bash
# The installer's install lock: the lines of scripts/install.sh between its
# `# >>> install lock` and `# <<< install lock` markers, run as installers
# run them, in separate /bin/bash processes with `set -euo pipefail`. Covers
# the lock location, waiting, a lock file that is removed or replaced while
# an installer waits for it, each lock tool, the fail-closed paths, and the
# order of the installer's EXIT cleanup. The uninstaller's side of the lock
# is tested in tests/log_permissions_and_uninstall.sh.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
INSTALL_SCRIPT="${PROJECT_DIR}/scripts/install.sh"
UNINSTALL_SCRIPT="${PROJECT_DIR}/scripts/uninstall.sh"
RAW_TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-install-lock-test.XXXXXX")"
TEMP_ROOT="$(cd "${RAW_TEMP_ROOT}" && /bin/pwd -P)"
LOCK_PROCESS_PIDS=()
cleanup() {
    # Closing the write ends of their pipes releases every lock process and
    # the lingering child.
    exec 4>&- 5>&- 6>&- 7>&-
    local pid
    for pid in ${LOCK_PROCESS_PIDS[@]+"${LOCK_PROCESS_PIDS[@]}"}; do
        wait "${pid}" 2>/dev/null || true
    done
    /bin/rm -rf -- "${TEMP_ROOT}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# The fakes' paths are spliced into sed programs below, which only works for
# plain path characters.
case "${TEMP_ROOT}" in
    *[!A-Za-z0-9/._+-]*) fail "TMPDIR must be a path of letters, digits, /, ., _, +, and -: ${TEMP_ROOT}" ;;
esac

CURRENT_UID="$(/usr/bin/id -u)"
LOCK_NAME="codex_monitor_install_${CURRENT_UID}.lock"
FAKE_BIN="${TEMP_ROOT}/bin"
MISSING="${TEMP_ROOT}/missing"
DARWIN_TMP="${TEMP_ROOT}/darwin-user-tmp"
LOCK="${DARWIN_TMP}/${LOCK_NAME}"
/bin/mkdir -p "${FAKE_BIN}" "${DARWIN_TMP}" "${TEMP_ROOT}/tmp-a" "${TEMP_ROOT}/tmp-b"
/bin/chmod 700 "${DARWIN_TMP}"

# What this host offers the lock, for the log of every run.
/usr/bin/printf 'install lock tools: /usr/bin/lockf %s; /usr/bin/perl %s; macOS %s\n' \
    "$([ -x /usr/bin/lockf ] && echo present || echo missing)" \
    "$([ -x /usr/bin/perl ] && echo present || echo missing)" \
    "$(/usr/bin/sw_vers -productVersion 2>/dev/null || echo unknown)"

# `getconf DARWIN_USER_TEMP_DIR` names the per-user temporary directory. The
# fake prints GETCONF_OUTPUT, or fails when it is absent.
GETCONF_OUTPUT="${TEMP_ROOT}/getconf-output"
/usr/bin/printf '%s/\n' "${DARWIN_TMP}" > "${GETCONF_OUTPUT}"
/bin/cat > "${FAKE_BIN}/getconf" <<EOF
#!/bin/sh
[ "\$#" -eq 1 ] && [ "\$1" = DARWIN_USER_TEMP_DIR ] && [ -f '${GETCONF_OUTPUT}' ] || exit 1
/bin/cat '${GETCONF_OUTPUT}'
EOF
# A lockf from before the descriptor form rejects `lockf -s -t 0 <fd>`.
OLD_LOCKF_CALLS="${TEMP_ROOT}/old-lockf-calls"
/bin/cat > "${FAKE_BIN}/old-lockf" <<EOF
#!/bin/sh
/usr/bin/printf '%s\n' "\$*" >> '${OLD_LOCKF_CALLS}'
echo 'usage: lockf [-kns] [-t seconds] file command [arguments]' >&2
exit 64
EOF
# Takes nothing and reports success, after replacing the lock file with a new
# one, so the path never names the file just "locked".
/bin/cat > "${FAKE_BIN}/replacing-lockf" <<EOF
#!/bin/sh
/usr/bin/printf 'replacement\n' > '${LOCK}.new' && /bin/mv -f '${LOCK}.new' '${LOCK}'
EOF
# Must never run: cleanup() opens the installed app only after quiescing
# writers, which the cleanup test does not do.
OPEN_CALLS="${TEMP_ROOT}/open-calls"
/bin/cat > "${FAKE_BIN}/open" <<EOF
#!/bin/sh
/usr/bin/printf '%s\n' "\$*" >> '${OPEN_CALLS}'
EOF
# Replaces the lock file right after reporting the path's identity, which
# the installer reads last before writing its PID.
SWAPPED="${TEMP_ROOT}/swapped"
/bin/cat > "${FAKE_BIN}/swapping-stat" <<EOF
#!/bin/sh
/usr/bin/stat "\$@"
status=\$?
case "\$*" in
    *' ${LOCK}')
        /usr/bin/printf 'swapped\n' > '${LOCK}.next' && /bin/mv -f '${LOCK}.next' '${LOCK}'
        /usr/bin/touch '${SWAPPED}'
        ;;
esac
exit "\${status}"
EOF
/bin/chmod 755 "${FAKE_BIN}/getconf" "${FAKE_BIN}/old-lockf" "${FAKE_BIN}/replacing-lockf" "${FAKE_BIN}/open" \
    "${FAKE_BIN}/swapping-stat"

# ------------------------------------------------------------------------------
# The block under test, exactly as install.sh has it, and variants with one
# lock tool replaced. It must only define functions and variables, because
# the tests source it; install.sh's own call follows the end marker.
# ------------------------------------------------------------------------------
BLOCK_RAW="${TEMP_ROOT}/install-lock-block.raw"
/usr/bin/awk '
    $0 == "# >>> install lock" { inside = 1; starts++; next }
    $0 == "# <<< install lock" { inside = 0; ends++; next }
    inside { print }
    END { exit (starts == 1 && ends == 1) ? 0 : 1 }
' "${INSTALL_SCRIPT}" > "${BLOCK_RAW}" || fail 'install.sh must have exactly one install lock block between its markers'
TOP_LEVEL_COMMANDS="$(/usr/bin/grep -n -v -E \
    -e '^[[:space:]]' -e '^$' -e '^#' -e '^[a-z_]+\(\) \{$' -e '^\}$' -e '^[A-Z_]+=' "${BLOCK_RAW}" || true)"
[ -z "${TOP_LEVEL_COMMANDS}" ] || fail "the install lock block runs commands when sourced:
${TOP_LEVEL_COMMANDS}"
/usr/bin/awk '
    $0 == "# <<< install lock" { after = 1; next }
    after && NF { found = ($0 == "acquire_install_lock || exit 1"); exit }
    END { exit found ? 0 : 1 }
' "${INSTALL_SCRIPT}" || fail 'install.sh must call acquire_install_lock || exit 1 right after the lock block'

make_block() {
    local copy="$1"
    shift
    /usr/bin/sed -e "s|/usr/bin/getconf|${FAKE_BIN}/getconf|g" "$@" "${BLOCK_RAW}" > "${copy}"
}
BLOCK_HOST="${TEMP_ROOT}/block-host.sh"
BLOCK_PERL="${TEMP_ROOT}/block-perl.sh"
BLOCK_OLD_LOCKF="${TEMP_ROOT}/block-old-lockf.sh"
BLOCK_LOCKF="${TEMP_ROOT}/block-lockf.sh"
BLOCK_NONE="${TEMP_ROOT}/block-none.sh"
BLOCK_REPLACING="${TEMP_ROOT}/block-replacing.sh"
BLOCK_SWAPPING_STAT="${TEMP_ROOT}/block-swapping-stat.sh"
make_block "${BLOCK_HOST}"
make_block "${BLOCK_PERL}" -e "s|/usr/bin/lockf|${MISSING}/lockf|g"
make_block "${BLOCK_OLD_LOCKF}" -e "s|/usr/bin/lockf|${FAKE_BIN}/old-lockf|g"
make_block "${BLOCK_LOCKF}" -e "s|/usr/bin/perl|${MISSING}/perl|g"
make_block "${BLOCK_NONE}" -e "s|/usr/bin/lockf|${MISSING}/lockf|g" -e "s|/usr/bin/perl|${MISSING}/perl|g"
make_block "${BLOCK_REPLACING}" -e "s|/usr/bin/lockf|${FAKE_BIN}/replacing-lockf|g"
make_block "${BLOCK_SWAPPING_STAT}" -e "s|/usr/bin/stat|${FAKE_BIN}/swapping-stat|g"
for name in lockf perl stat; do
    /usr/bin/grep -F "/usr/bin/${name}" "${BLOCK_HOST}" >/dev/null ||
        fail "the install lock block no longer names /usr/bin/${name}, which this test replaces"
done

# The copies may reach the per-user temporary directory only through the fake
# getconf, and must not read or signal processes or source a file; otherwise
# this test would lock, or leave a file in, the real temporary directory.
SYSTEM_COMMAND='(^|[^[:alnum:]_.])(getconf|kill|pkill|killall|ps|pgrep|launchctl|osascript)([^[:alnum:]_.-]|$)|(^|[^[:alnum:]_.-])open[[:space:]]'
SOURCE_COMMAND='(^|[;&|({[:space:]])(source|[.])[[:space:]]'
unfaked_commands() {
    /usr/bin/sed -e "s|${FAKE_BIN}/[A-Za-z0-9_-]*||g" "$1" |
        /usr/bin/grep -n -E "${SYSTEM_COMMAND}|${SOURCE_COMMAND}" |
        /usr/bin/grep -v -E '^[0-9]+:[[:space:]]*#' || true
}
PROBE_LINES="${TEMP_ROOT}/guard-probe.sh"
for line in 'dir="$(/usr/bin/getconf DARWIN_USER_TEMP_DIR)"' 'getconf DARWIN_USER_TEMP_DIR' \
    'kill -0 "${holder_pid}"' '/bin/ps -p 1' '/usr/bin/open "${app}"' 'source "${x}"' '    . ./x.sh'; do
    /usr/bin/printf '%s\n' "${line}" > "${PROBE_LINES}"
    [ -n "$(unfaked_commands "${PROBE_LINES}")" ] || fail "fake guard missed: ${line}"
done
/usr/bin/printf '%s\n' "\"${FAKE_BIN}/getconf\" DARWIN_USER_TEMP_DIR" '# getconf kill ps' \
    'open(my $lock, "<&=", $ARGV[0]) or exit 71;' > "${PROBE_LINES}"
[ -z "$(unfaked_commands "${PROBE_LINES}")" ] || fail 'fake guard rejected a fake or a comment'
for copy in "${BLOCK_HOST}" "${BLOCK_PERL}" "${BLOCK_OLD_LOCKF}" "${BLOCK_LOCKF}" \
    "${BLOCK_NONE}" "${BLOCK_REPLACING}" "${BLOCK_SWAPPING_STAT}"; do
    unfaked="$(unfaked_commands "${copy}")"
    [ -z "${unfaked}" ] || fail "install lock copy can still reach a real command: ${copy##*/}
${unfaked}"
done

# The installer and the uninstaller must take the same lock the same way.
flock_function() {
    /usr/bin/awk '$0 == "flock_fd_now() {" { inside = 1 } inside { print } inside && $0 == "}" { exit }' "$1"
}
[ -n "$(flock_function "${INSTALL_SCRIPT}")" ] &&
    [ "$(flock_function "${INSTALL_SCRIPT}")" = "$(flock_function "${UNINSTALL_SCRIPT}")" ] ||
    fail 'install.sh and uninstall.sh must keep identical flock_fd_now functions'
# lockf waits on a descriptor by spinning a CPU, so neither script may run it
# without `-t 0`.
BLOCKING_LOCKF="$(/usr/bin/grep -n -E '/usr/bin/lockf[[:space:]]+-' "${INSTALL_SCRIPT}" "${UNINSTALL_SCRIPT}" |
    /usr/bin/grep -v -E ':[[:space:]]*#' | /usr/bin/grep -v -F -e '/usr/bin/lockf -s -t 0 ' || true)"
[ -z "${BLOCKING_LOCKF}" ] || fail "a script can run lockf without -t 0:
${BLOCKING_LOCKF}"

# install.sh checks that it still holds its lock file right before each step
# that creates a path an uninstaller must not remove during an install.
for step in '    TMP_DIR="$(mktemp -d -t codex-mon-install-XXXXXX)"' \
    'CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"' \
    'prepare_app_bundle_staging "${APP_DIR}" "${INSTALL_DIR}" "${BUNDLE_NAME}"' \
    'activate_app_bundle_staging "${INSTALL_DIR}/${BUNDLE_NAME}"'; do
    /usr/bin/awk -v step="${step}" '
        $0 == step { steps++; if (previous ~ /^[[:space:]]*install_lock_still_named \|\| exit 1$/) checked++ }
        { previous = $0 }
        END { exit (steps == 1 && checked == 1) ? 0 : 1 }
    ' "${INSTALL_SCRIPT}" || fail "install.sh must run install_lock_still_named || exit 1 right before: ${step}"
done

# ------------------------------------------------------------------------------
# Lock processes. Each runs one block copy in /bin/bash like install.sh:
# acquire_install_lock || exit 1, then records the identity of the file it
# locked and holds the lock until the write end of its pipe (descriptor 5, 6,
# or 7 of this test) closes, which also happens if this test dies. Children
# never inherit those write ends.
# ------------------------------------------------------------------------------
LOCK_PROCESS="${TEMP_ROOT}/lock-process.sh"
/bin/cat > "${LOCK_PROCESS}" <<'EOF'
#!/bin/bash
set -euo pipefail
source "$1"
INSTALL_LOCK_POLL_SECONDS=0.05
acquire_install_lock || exit 1
/usr/bin/printf '%s\n' "${INSTALL_LOCK_IDENTITY}" > "$2.identity"
if [ -n "${3:-}" ]; then
    # A command the installer starts that outlives it, such as a compiler
    # cache server: it ends only when this test closes its pipe.
    /bin/bash -c 'read -r _ || true' < "$3" &
fi
/usr/bin/touch "$2.acquired"
read -r _ || true
EOF

# start_lock_process NAME FD BLOCK TMPDIR [LINGER_FIFO]
start_lock_process() {
    local name="$1" fd="$2" block="$3" tmpdir="$4" linger="${5:-}"
    local base="${TEMP_ROOT}/${name}"
    /bin/rm -f "${base}.fifo" "${base}.identity" "${base}.acquired" "${base}.log"
    /usr/bin/mkfifo "${base}.fifo"
    TMPDIR="${tmpdir}" /bin/bash "${LOCK_PROCESS}" "${block}" "${base}" "${linger}" \
        < "${base}.fifo" > "${base}.log" 2>&1 4>&- 5>&- 6>&- 7>&- &
    LOCK_PROCESS_PIDS+=("$!")
    eval "${name}_PID=\$!"
    eval "exec ${fd}>\"\${base}.fifo\""
}

# release_lock_process NAME FD: ends it and requires a successful exit.
release_lock_process() {
    local name="$1" fd="$2" pid
    eval "exec ${fd}>&-"
    eval "pid=\${${name}_PID}"
    wait "${pid}" || fail "lock process ${name} failed: $(/bin/cat "${TEMP_ROOT}/${name}.log")"
}

# bounded SECONDS COMMAND...: runs COMMAND, killed by SIGALRM (status 142) if
# it is still running after SECONDS, so a regression that loops fails
# instead of hanging the test.
bounded() {
    local seconds="$1"
    shift
    /usr/bin/perl -e 'alarm shift; exec @ARGV or exit 127' "${seconds}" "$@"
}

# wait_until WHAT COMMAND...: polls COMMAND for up to 10 seconds.
wait_until() {
    local what="$1"
    shift
    local attempt=0
    until "$@"; do
        attempt=$((attempt + 1))
        [ "${attempt}" -lt 400 ] || fail "timed out waiting until ${what}"
        /bin/sleep 0.025
    done
}
acquired() { [ -e "${TEMP_ROOT}/$1.acquired" ]; }
lock_is() { [ "$(probe "${LOCK}")" = "$1" ]; }
logged() { /usr/bin/grep -F -- "$2" "${TEMP_ROOT}/$1.log" >/dev/null 2>&1; }
identity_of() { /bin/cat "${TEMP_ROOT}/$1.identity"; }
path_identity() { /usr/bin/stat -f '%u %d:%i %HT' "$1" 2>/dev/null || echo absent; }

# An independent probe of the file at a path: prints held, free, or absent.
probe() {
    local status=0
    /usr/bin/perl -MErrno -MFcntl=:flock -e '
        open(my $lock, "<", $ARGV[0]) or exit($!{ENOENT} ? 69 : 71);
        flock($lock, LOCK_EX | LOCK_NB) or exit($!{EWOULDBLOCK} ? 75 : 71);
        exit 0;
    ' "$1" || status=$?
    case "${status}" in
        0) echo free ;;
        69) echo absent ;;
        75) echo held ;;
        *) echo "unknown (${status})" ;;
    esac
}

# The lock process NAME holds the file that the lock path names now.
assert_holds_named_lock() {
    local name="$1"
    [ "$(identity_of "${name}")" = "$(path_identity "${LOCK}")" ] ||
        fail "${name} holds a lock file the path no longer names: locked $(identity_of "${name}"), path $(path_identity "${LOCK}")"
    [ "$(probe "${LOCK}")" = held ] || fail "${name}'s lock file is not locked: $(probe "${LOCK}")"
}

# ------------------------------------------------------------------------------
# The lock file is the per-user temporary directory's, whatever TMPDIR says,
# so installers (and uninstallers) started from different environments meet.
# A waiter names the holder, and the holder's PID is the file's first line
# even over a longer earlier line.
# ------------------------------------------------------------------------------
/usr/bin/printf '123456789012345678901234567890\nstale second line\n' > "${LOCK}"
start_lock_process H 5 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-a"
wait_until 'H holds the lock' acquired H
[ ! -e "${TEMP_ROOT}/tmp-a/${LOCK_NAME}" ] ||
    fail 'the installer locked a file in its TMPDIR instead of the per-user temporary directory'
assert_holds_named_lock H
[ "$(/usr/bin/head -n 1 "${LOCK}")" = "${H_PID}" ] ||
    fail "the lock file does not start with the holder's PID: $(/usr/bin/head -n 1 "${LOCK}")"
start_lock_process W 6 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-b"
wait_until 'W waits for H' logged W "Another installation (PID ${H_PID}) is currently in progress"
acquired W && fail 'W took the lock while H held it'
[ ! -e "${TEMP_ROOT}/tmp-b/${LOCK_NAME}" ] || fail 'a waiting installer created a lock file in TMPDIR'
release_lock_process H 5
wait_until 'W takes the lock after H' acquired W
assert_holds_named_lock W
logged W 'Acquired installation lock' || fail 'W did not report taking the lock'
logged W 'replaced' && fail 'W reported a replaced lock file although none was'
release_lock_process W 6
# Each installer's lock keeper exits right after its installer.
wait_until 'the lock is free after both installers exited' lock_is free

# ------------------------------------------------------------------------------
# A command the installer started that outlives it, such as a compiler cache
# server, must not keep the lock: the installer hands the lock to a keeper
# that exits with it, so no command it runs inherits the lock.
# ------------------------------------------------------------------------------
LINGER="${TEMP_ROOT}/linger.fifo"
/usr/bin/mkfifo "${LINGER}"
start_lock_process H 5 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-a" "${LINGER}"
exec 4>"${LINGER}"
wait_until 'H holds the lock' acquired H
lock_is held || fail "H's lock is not held: $(probe "${LOCK}")"
release_lock_process H 5
wait_until 'the lock is free while a command the installer started still runs' lock_is free
exec 4>&-


# ------------------------------------------------------------------------------
# A waiter whose lock file is removed while it waits (the uninstaller removes
# a free lock file while holding its lock) must lock the file the path names
# afterwards, not keep the removed one, which nobody else can see.
# ------------------------------------------------------------------------------
start_lock_process H 5 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-a"
wait_until 'H holds the lock' acquired H
REMOVED_IDENTITY="$(path_identity "${LOCK}")"
start_lock_process W 6 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-b"
wait_until 'W waits for H' logged W 'is currently in progress'
/bin/rm -f "${LOCK}"
release_lock_process H 5
wait_until 'W takes a lock' acquired W
assert_holds_named_lock W
[ "$(identity_of W)" != "${REMOVED_IDENTITY}" ] || fail 'W kept the removed lock file'
logged W 'The install lock file was replaced while waiting' || fail 'W did not report the replaced lock file'
release_lock_process W 6

# ------------------------------------------------------------------------------
# Meanwhile a second installer can create and lock a new file at the path.
# The first waiter must then wait for that installer instead of running
# alongside it.
# ------------------------------------------------------------------------------
start_lock_process H 5 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-a"
wait_until 'H holds the lock' acquired H
start_lock_process W 6 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-b"
wait_until 'W waits for H' logged W 'is currently in progress'
/bin/rm -f "${LOCK}"
start_lock_process W2 7 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-a"
wait_until 'W2 locks a new lock file' acquired W2
assert_holds_named_lock W2
release_lock_process H 5
wait_until 'W finds the lock file replaced' logged W 'The install lock file was replaced while waiting'
acquired W && fail 'W took a lock while W2 held the current lock file'
release_lock_process W2 7
wait_until 'W takes the lock after W2' acquired W
assert_holds_named_lock W
release_lock_process W 6

# ------------------------------------------------------------------------------
# Taking the lock leaves the installer's own signal traps as they were: the
# keeper is forked with the signals ignored, and the traps restored after.
# ------------------------------------------------------------------------------
TRAPS="$(bounded 60 /bin/bash -c 'set -euo pipefail; trap "exit 130" INT; trap "exit 143" TERM
    source "$1"; acquire_install_lock >/dev/null || exit 1; trap -p INT TERM HUP QUIT' traps "${BLOCK_HOST}")" ||
    fail 'the trap check could not take the lock'
[ "${TRAPS}" = "trap -- 'exit 130' SIGINT
trap -- 'exit 143' SIGTERM" ] || fail "taking the lock changed the installer's traps:
${TRAPS}"
wait_until 'the lock is free after the trap check' lock_is free

# ------------------------------------------------------------------------------
# An installer stops before creating a path once another program has removed
# or replaced its lock file (uninstallers from before this lock removed it
# again after releasing it).
# ------------------------------------------------------------------------------
NAMED_OUTPUT="${TEMP_ROOT}/named.log"
bounded 60 /bin/bash -c 'set -euo pipefail; source "$1"; acquire_install_lock || exit 1
    install_lock_still_named || exit 3
    /usr/bin/printf "replacement\n" > "${INSTALL_LOCK_FILE}.next"
    /bin/mv -f "${INSTALL_LOCK_FILE}.next" "${INSTALL_LOCK_FILE}"
    if install_lock_still_named; then exit 4; fi
    /bin/rm -f "${INSTALL_LOCK_FILE}"
    if install_lock_still_named; then exit 5; fi' named "${BLOCK_HOST}" > "${NAMED_OUTPUT}" 2>&1 ||
    fail "install_lock_still_named did not track the lock file: $(/bin/cat "${NAMED_OUTPUT}")"
[ "$(/usr/bin/grep -c -F 'another program removed or replaced the install lock file' "${NAMED_OUTPUT}")" = 2 ] ||
    fail "install_lock_still_named did not report the lost lock file: $(/bin/cat "${NAMED_OUTPUT}")"

# ------------------------------------------------------------------------------
# A new lock file is private whatever the umask, and a lock file this user
# can only read is still locked (flock needs no write access).
# ------------------------------------------------------------------------------
/bin/rm -f "${LOCK}"
bounded 60 /bin/bash -c 'set -euo pipefail; umask 0277; source "$1"; acquire_install_lock || exit 1' umask \
    "${BLOCK_HOST}" > "${TEMP_ROOT}/umask.log" 2>&1 ||
    fail "the installer failed under umask 0277: $(/bin/cat "${TEMP_ROOT}/umask.log")"
[ "$(/usr/bin/stat -f '%Lp' "${LOCK}")" = 600 ] ||
    fail "a new lock file is not private: $(/usr/bin/stat -f '%Lp' "${LOCK}")"
wait_until 'the lock is free after the umask run' lock_is free
/bin/chmod 400 "${LOCK}"
start_lock_process H 5 "${BLOCK_HOST}" "${TEMP_ROOT}/tmp-a"
wait_until 'H holds a read-only lock file' acquired H
assert_holds_named_lock H
release_lock_process H 5
/bin/rm -f "${LOCK}"

# ------------------------------------------------------------------------------
# The PID goes through the locked descriptor. Written by path, it would land
# in, or create, a file that is not locked, if the path changed after the
# installer checked it (here right after that check).
# ------------------------------------------------------------------------------
/bin/rm -f "${SWAPPED}"
start_lock_process H 5 "${BLOCK_SWAPPING_STAT}" "${TEMP_ROOT}/tmp-a"
wait_until 'H holds the lock' acquired H
[ -e "${SWAPPED}" ] || fail 'the installer never compared the lock path with the file it locked'
[ "$(/bin/cat "${LOCK}")" = swapped ] ||
    fail "the installer wrote its PID by path into a file it does not lock: $(/bin/cat "${LOCK}")"
release_lock_process H 5
/bin/rm -f "${LOCK}"

# ------------------------------------------------------------------------------
# Each lock tool takes the same lock. macOS 13 and 14 have no lockf, so perl
# locks there; a lockf without the descriptor form falls back to perl.
# ------------------------------------------------------------------------------
start_lock_process H 5 "${BLOCK_PERL}" "${TEMP_ROOT}/tmp-a"
wait_until 'perl takes the lock' acquired H
assert_holds_named_lock H
release_lock_process H 5

start_lock_process H 5 "${BLOCK_OLD_LOCKF}" "${TEMP_ROOT}/tmp-a"
wait_until 'perl takes the lock after an old lockf' acquired H
assert_holds_named_lock H
[ "$(/bin/cat "${OLD_LOCKF_CALLS}")" = '-s -t 0 9' ] || fail 'the old lockf was not tried first'
release_lock_process H 5

if [ -x /usr/bin/lockf ]; then
    # Both directions across tools: perl holds while lockf waits, and back.
    start_lock_process H 5 "${BLOCK_PERL}" "${TEMP_ROOT}/tmp-a"
    wait_until 'perl takes the lock' acquired H
    start_lock_process W 6 "${BLOCK_LOCKF}" "${TEMP_ROOT}/tmp-b"
    wait_until 'lockf waits for perl' logged W 'is currently in progress'
    release_lock_process H 5
    wait_until 'lockf takes the lock' acquired W
    assert_holds_named_lock W
    start_lock_process W2 7 "${BLOCK_PERL}" "${TEMP_ROOT}/tmp-a"
    wait_until 'perl waits for lockf' logged W2 'is currently in progress'
    release_lock_process W 6
    wait_until 'perl takes the lock after lockf' acquired W2
    release_lock_process W2 7
else
    printf 'install lock: the lockf path needs /usr/bin/lockf (macOS 15 and later) and was not run\n'
fi

# ------------------------------------------------------------------------------
# Fail closed: an installer that cannot take or verify the lock must stop,
# because the uninstaller would treat its temporary paths as abandoned.
# ------------------------------------------------------------------------------
# run_refused WHAT BLOCK MESSAGE: acquire_install_lock must fail with MESSAGE.
run_refused() {
    local what="$1" block="$2" message="$3" output="${TEMP_ROOT}/refused.log"
    local status=0
    TMPDIR="${TEMP_ROOT}/tmp-a" bounded 60 /bin/bash -c 'set -euo pipefail; source "$1"
        INSTALL_LOCK_POLL_SECONDS=0.05; acquire_install_lock || exit 1; echo LOCKED' \
        refused "${block}" > "${output}" 2>&1 || status=$?
    [ "${status}" -ne 0 ] || fail "installer continued ${what}: $(/bin/cat "${output}")"
    [ "${status}" -ne 142 ] || fail "installer never gave up ${what}"
    /usr/bin/grep -F -- "${message}" "${output}" >/dev/null ||
        fail "installer refused ${what} without saying why: $(/bin/cat "${output}")"
    /usr/bin/grep -F -x LOCKED "${output}" >/dev/null && fail "installer locked ${what}"
    [ ! -e "${TEMP_ROOT}/tmp-a/${LOCK_NAME}" ] || fail "installer fell back to TMPDIR ${what}"
    return 0
}
run_refused 'without lockf or perl' "${BLOCK_NONE}" 'Refusing installation: the install lock needs '
run_refused 'while the lock file kept being replaced' "${BLOCK_REPLACING}" 'the install lock file kept changing'
/bin/rm -f "${LOCK}"
/bin/rm -f "${GETCONF_OUTPUT}"
run_refused 'without a per-user temporary directory' "${BLOCK_HOST}" 'temporary directory (DARWIN_USER_TEMP_DIR) is unavailable'
/usr/bin/printf 'relative/\n' > "${GETCONF_OUTPUT}"
run_refused 'with a relative per-user temporary directory' "${BLOCK_HOST}" 'temporary directory (DARWIN_USER_TEMP_DIR) is unavailable'
/usr/bin/printf '%s/\n' "${TEMP_ROOT}/no-such-dir" > "${GETCONF_OUTPUT}"
run_refused 'with a missing per-user temporary directory' "${BLOCK_HOST}" 'temporary directory (DARWIN_USER_TEMP_DIR) is unavailable'
/bin/ln -s "${DARWIN_TMP}" "${TEMP_ROOT}/darwin-tmp-link"
/usr/bin/printf '%s/\n' "${TEMP_ROOT}/darwin-tmp-link" > "${GETCONF_OUTPUT}"
run_refused 'through a symlinked per-user temporary directory' "${BLOCK_HOST}" 'temporary directory (DARWIN_USER_TEMP_DIR) is unavailable'
/usr/bin/printf '%s/\n' "${DARWIN_TMP}" > "${GETCONF_OUTPUT}"
[ ! -e "${LOCK}" ] || fail 'a refused installer left a lock file'

/usr/bin/printf 'symlink target sentinel\n' > "${TEMP_ROOT}/lock-target"
/bin/ln -s "${TEMP_ROOT}/lock-target" "${LOCK}"
run_refused 'through a symlinked lock file' "${BLOCK_HOST}" 'the install lock is not a regular file'
[ "$(/bin/cat "${TEMP_ROOT}/lock-target")" = 'symlink target sentinel' ] ||
    fail 'the installer wrote through a symlinked lock file'
/bin/rm -f "${LOCK}"
/bin/mkdir "${LOCK}"
run_refused 'with a directory as its lock file' "${BLOCK_HOST}" 'the install lock is not a regular file'
/bin/rmdir "${LOCK}"
/usr/bin/touch "${LOCK}"
/bin/chmod 000 "${LOCK}"
[ ! -r "${LOCK}" ] || fail 'this test cannot make an unreadable file; do not run it as root'
run_refused 'with an unreadable lock file' "${BLOCK_HOST}" 'the install lock cannot be opened'
/bin/rm -f "${LOCK}"

# ------------------------------------------------------------------------------
# install.sh's EXIT cleanup removes every temporary path, codesign's .cstemp
# copy included, while it still holds the lock: the uninstaller treats a free
# lock as proof that no installer owns them. The cleanup function runs as
# written, with the app-bundle helpers and rm recording the lock state.
# ------------------------------------------------------------------------------
CLEANUP_RAW="${TEMP_ROOT}/cleanup-function.sh"
/usr/bin/awk '$0 == "cleanup() {" { inside = 1; starts++ } inside { print } inside && $0 == "}" { inside = 0 }
    END { exit starts == 1 ? 0 : 1 }' "${INSTALL_SCRIPT}" > "${CLEANUP_RAW}" ||
    fail 'install.sh must define cleanup() exactly once'
CLEANUP_COPY="${TEMP_ROOT}/cleanup-copy.sh"
/usr/bin/sed -e "s|/usr/bin/open|${FAKE_BIN}/open|g" "${CLEANUP_RAW}" > "${CLEANUP_COPY}"
UNFAKED_CLEANUP="$(unfaked_commands "${CLEANUP_COPY}")"
[ -z "${UNFAKED_CLEANUP}" ] || fail "the cleanup copy can still reach a real command:
${UNFAKED_CLEANUP}"
CLEANUP_LOG="${TEMP_ROOT}/cleanup.log"
STAGING="${TEMP_ROOT}/local-bin/.codex-mon.install.Ab3dE9"
CLONE="${DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D4e5"
/bin/mkdir -p "${TEMP_ROOT}/local-bin" "${CLONE}/.git"
/usr/bin/touch "${STAGING}" "${STAGING}.cstemp" "${CLONE}/Cargo.toml"
CLEANUP_PROCESS="${TEMP_ROOT}/cleanup-process.sh"
/bin/cat > "${CLEANUP_PROCESS}" <<'EOF'
#!/bin/bash
set -euo pipefail
source "$1"
source "$2"
CLEANUP_LOG="$3"
CLI_STAGING="$4"
TMP_DIR="$5"
CLEANUP_TMP=1
APP_SWAP_ACTIVE=1
INSTALL_SUCCEEDED=0
WRITERS_QUIESCED=0
lock_state() {
    local status=0
    /usr/bin/perl -MErrno -MFcntl=:flock -e '
        open(my $lock, "<", $ARGV[0]) or exit 71;
        flock($lock, LOCK_EX | LOCK_NB) or exit($!{EWOULDBLOCK} ? 75 : 71);
        exit 0;
    ' "${INSTALL_LOCK_FILE}" || status=$?
    if [ "${status}" -eq 75 ]; then echo held; else echo "not-held-${status}"; fi
}
rm() {
    /usr/bin/printf 'rm %s %s\n' "$(lock_state)" "$*" >> "${CLEANUP_LOG}"
    /bin/rm "$@"
}
rollback_app_bundle_swap() { /usr/bin/printf 'rollback %s\n' "$(lock_state)" >> "${CLEANUP_LOG}"; }
cleanup_app_bundle_swap_paths() { /usr/bin/printf 'bundle-cleanup %s\n' "$(lock_state)" >> "${CLEANUP_LOG}"; }
# install.sh sets its traps before it takes the lock; taking it must keep them.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
acquire_install_lock || exit 1
if [ "${6:-}" = interrupted ]; then
    # As a terminal's Ctrl-C or a group SIGTERM would: this process group
    # (the test puts it in its own) holds only this installer and its keeper.
    kill -TERM 0
fi
exit 3
EOF
EXPECTED_CLEANUP_LOG="rm held -f ${STAGING} ${STAGING}.cstemp
rm held -rf ${CLONE}
rollback held
bundle-cleanup held"
# Once as the installer ends, and once as a signal to its whole process group
# ends it: the lock keeper ignores the signal and outlasts the cleanup.
for ending in exit interrupted; do
    /bin/rm -f "${CLEANUP_LOG}"
    /bin/mkdir -p "${CLONE}/.git"
    /usr/bin/touch "${STAGING}" "${STAGING}.cstemp" "${CLONE}/Cargo.toml"
    cleanup_status=0
    TMPDIR="${TEMP_ROOT}/tmp-a" /usr/bin/perl -e 'setpgrp(0, 0); alarm shift; exec @ARGV or exit 127' 60 \
        /bin/bash "${CLEANUP_PROCESS}" "${BLOCK_HOST}" "${CLEANUP_COPY}" "${CLEANUP_LOG}" "${STAGING}" \
        "${CLONE}" "${ending}" > "${TEMP_ROOT}/cleanup-output.log" 2>&1 || cleanup_status=$?
    expected_status=3
    [ "${ending}" = exit ] || expected_status=143
    [ "${cleanup_status}" -eq "${expected_status}" ] ||
        fail "cleanup test (${ending}) exited ${cleanup_status}: $(/bin/cat "${TEMP_ROOT}/cleanup-output.log")"
    [ "$(/bin/cat "${CLEANUP_LOG}")" = "${EXPECTED_CLEANUP_LOG}" ] ||
        fail "installer cleanup (${ending}) must remove every temporary path before releasing the lock:
$(/bin/cat "${CLEANUP_LOG}")"
    for path in "${STAGING}" "${STAGING}.cstemp" "${CLONE}"; do
        [ ! -e "${path}" ] || fail "installer cleanup (${ending}) left ${path}"
    done
    wait_until "the lock is free after the installer exited (${ending})" lock_is free
done
[ ! -e "${OPEN_CALLS}" ] || fail 'the cleanup test opened an app'

printf 'installer install lock: ok\n'
