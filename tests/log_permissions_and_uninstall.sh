#!/bin/bash
set -euo pipefail

# Every uninstaller run below sets HOME, and CODEX_HOME only where a case
# needs it: an inherited CODEX_HOME would point the runs at the caller's
# real Codex home.
unset CODEX_HOME

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RAW_TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/codex-monitor-log-test.XXXXXX")"
TEMP_ROOT="$(cd "${RAW_TEMP_ROOT}" && /bin/pwd -P)"
LOCK_HOLDER=""
cleanup() {
    # Closing the write end of its pipe releases a lock holder.
    exec 7>&-
    if [ -n "${LOCK_HOLDER}" ]; then
        wait "${LOCK_HOLDER}" 2>/dev/null || true
    fi
    /bin/rm -rf -- "${TEMP_ROOT}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

assert_mode() {
    local expected="$1"
    local path="$2"
    local actual
    actual="$(/usr/bin/stat -f '%Lp' "${path}")"
    [ "${actual}" = "${expected}" ] ||
        fail "expected mode ${expected} for ${path}, got ${actual}"
}

assert_exists() {
    [ -e "$1" ] || fail "expected path to exist: $1"
}

assert_absent() {
    [ ! -e "$1" ] && [ ! -L "$1" ] || fail "expected path to be absent: $1"
}

assert_content() {
    local expected="$1"
    local path="$2"
    local actual
    actual="$(/bin/cat "${path}")"
    [ "${actual}" = "${expected}" ] || fail "unexpected sentinel content: ${path}"
}

# Whether a line of the output file names exactly this path, followed by the
# end of the line or a space. A plain substring search would also match a
# look-alike whose path is a prefix of a valid fixture's path.
output_names_path() {
    local output="$1"
    local path="$2"
    /usr/bin/awk -v path="${path}" '
        {
            line = $0
            while ((at = index(line, path)) > 0) {
                next_char = substr(line, at + length(path), 1)
                if (next_char == "" || next_char == " ") { found = 1 }
                line = substr(line, at + 1)
            }
        }
        END { exit found ? 0 : 1 }' "${output}"
}

# ------------------------------------------------------------------------------
# Installer contract the uninstaller relies on. Uninstall removes interrupted
# installer staging only while no installer holds the install lock, so
# install.sh must create every temporary path only after acquiring that lock.
# That its EXIT cleanup removes them while still holding the lock is run and
# checked in tests/install_lock.sh. Every mktemp template in the installer
# must be one the uninstaller matches: a new or renamed template fails here
# until scripts/uninstall.sh and this test cover it.
# ------------------------------------------------------------------------------
INSTALL_SCRIPT="${PROJECT_DIR}/scripts/install.sh"

installer_line() {
    local expected="$1"
    local matches
    matches="$(/usr/bin/grep -n -F -x -- "${expected}" "${INSTALL_SCRIPT}" || true)"
    [ -n "${matches}" ] && [ "$(/usr/bin/printf '%s\n' "${matches}" | /usr/bin/wc -l | /usr/bin/tr -d ' ')" = 1 ] ||
        fail "expected exactly one installer line: ${expected}"
    /usr/bin/printf '%s\n' "${matches%%:*}"
}

lock_line="$(installer_line 'acquire_install_lock || exit 1')"
clone_line="$(installer_line '    TMP_DIR="$(mktemp -d -t codex-mon-install-XXXXXX)"')"
cli_staging_line="$(installer_line 'CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"')"
bundle_staging_line="$(installer_line 'prepare_app_bundle_staging "${APP_DIR}" "${INSTALL_DIR}" "${BUNDLE_NAME}"')"
[ "${lock_line}" -lt "${clone_line}" ] ||
    fail 'installer creates its remote clone before taking the install lock'
[ "${lock_line}" -lt "${cli_staging_line}" ] ||
    fail 'installer stages the CLI before taking the install lock'
[ "${lock_line}" -lt "${bundle_staging_line}" ] ||
    fail 'installer stages the app bundle before taking the install lock'

EXPECTED_INSTALLER_MKTEMPS="$(/usr/bin/sort <<'EOF'
TMP_DIR="$(mktemp -d -t codex-mon-install-XXXXXX)"
CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"
APP_STAGE_ROOT="$(/usr/bin/mktemp -d "${install_dir}/.codex-monitor-install.XXXXXX")" || return 1
APP_BACKUP_ROOT="$(/usr/bin/mktemp -d "${install_dir}/.codex-monitor-backup.XXXXXX")" || return 1
EOF
)"
# Scan install.sh and every script it sources or runs through
# ${PROJECT_DIR}. It only copies uninstall.sh into the app bundle.
INSTALLER_SCRIPTS=("${INSTALL_SCRIPT}")
while IFS= read -r relative; do
    [ "${relative}" != scripts/uninstall.sh ] || continue
    [ -f "${PROJECT_DIR}/${relative}" ] || fail "installer references a missing script: ${relative}"
    INSTALLER_SCRIPTS+=("${PROJECT_DIR}/${relative}")
done < <(/usr/bin/grep -o '[$][{]PROJECT_DIR[}]/[A-Za-z0-9_./-]*[.]sh' "${INSTALL_SCRIPT}" |
    /usr/bin/sed -e 's|^[$][{]PROJECT_DIR[}]/||' | /usr/bin/sort -u)
for relative in scripts/install_bundle_swap.sh scripts/wait_for_restart_worker.sh codex-notifier/build.sh; do
    case " ${INSTALLER_SCRIPTS[*]} " in
        *" ${PROJECT_DIR}/${relative} "*) ;;
        *) fail "installer script scan missed ${relative}" ;;
    esac
done
ACTUAL_INSTALLER_MKTEMPS="$(/bin/cat "${INSTALLER_SCRIPTS[@]}" | /usr/bin/grep -F 'mktemp' |
    /usr/bin/grep -v -E '^[[:space:]]*#' | /usr/bin/sed -e 's/^[[:space:]]*//' | /usr/bin/sort)"
[ "${ACTUAL_INSTALLER_MKTEMPS}" = "${EXPECTED_INSTALLER_MKTEMPS}" ] ||
    fail "installer mktemp templates changed; update scripts/uninstall.sh and this test:
${ACTUAL_INSTALLER_MKTEMPS}"

# The uninstaller's own copies: it edits exactly these three files, each
# through one mktemp template. The fixtures below are made with that
# template, so uninstaller_temps must match what the uninstaller creates.
UNINSTALL_SCRIPT="${PROJECT_DIR}/scripts/uninstall.sh"
UNINSTALL_MKTEMP='temporary="$(/usr/bin/mktemp "${path}.codex-monitor-uninstall.XXXXXX" 2>/dev/null || true)"'
ACTUAL_UNINSTALLER_MKTEMPS="$(/usr/bin/grep -F 'mktemp' "${UNINSTALL_SCRIPT}" |
    /usr/bin/grep -v -E '^[[:space:]]*#' | /usr/bin/sed -e 's/^[[:space:]]*//' | /usr/bin/sort -u)"
[ "${ACTUAL_UNINSTALLER_MKTEMPS}" = "${UNINSTALL_MKTEMP}" ] ||
    fail "uninstaller mktemp templates changed; update uninstaller_temps and this test:
${ACTUAL_UNINSTALLER_MKTEMPS}"
UNINSTALL_COPY_SUFFIX="$(/usr/bin/printf '%s\n' "${UNINSTALL_MKTEMP}" | /usr/bin/sed -e 's/.*"[$][{]path[}]\([^"]*\)".*/\1/')"
ACTUAL_EDITED_FILES="$(/usr/bin/grep -E '^clean_rc_file "|^    local path="[$][{]USER_HOME[}]/[.]codex/config[.]toml"$' "${UNINSTALL_SCRIPT}")"
[ "${ACTUAL_EDITED_FILES}" = '    local path="${USER_HOME}/.codex/config.toml"
clean_rc_file "${USER_HOME}/.zshrc"
clean_rc_file "${USER_HOME}/.bash_profile"' ] ||
    fail "the uninstaller edits other files; update uninstaller_temps and this test:
${ACTUAL_EDITED_FILES}"

# Exercise the same fd-anchored Rust helper used by install.sh and uninstall.sh.
# The fake homes are canonicalized so the test does not reject macOS's /var
# compatibility symlink as an unsafe parent component.
# Build from the crate directory so rustup applies codex-switcher/rust-toolchain.toml;
# --manifest-path from elsewhere would use the caller's default toolchain.
(cd "${PROJECT_DIR}/codex-switcher" && /usr/bin/env cargo build --locked --quiet --bin codex-mon)
MONITOR_BIN="${PROJECT_DIR}/codex-switcher/target/debug/codex-mon"
[ -x "${MONITOR_BIN}" ] || fail 'codex-mon helper was not built'

install_fake_helper() {
    local home="$1"
    /bin/mkdir -p "${home}/.local/bin"
    /bin/cp "${MONITOR_BIN}" "${home}/.local/bin/codex-mon"
    /bin/chmod 755 "${home}/.local/bin/codex-mon"
}

INSTALL_HOME="${TEMP_ROOT}/installer-home"
/bin/mkdir -p "${INSTALL_HOME}/.codex/log/archive" "${INSTALL_HOME}/.codex/recovery-runs"
/usr/bin/touch "${INSTALL_HOME}/.codex/account-switcher-daemon.log" "${INSTALL_HOME}/.codex/account-switcher-daemon.err"
/bin/chmod 755 "${INSTALL_HOME}/.codex" "${INSTALL_HOME}/.codex/log" \
    "${INSTALL_HOME}/.codex/log/archive" "${INSTALL_HOME}/.codex/recovery-runs"
/bin/chmod 644 "${INSTALL_HOME}/.codex/account-switcher-daemon.log" \
    "${INSTALL_HOME}/.codex/account-switcher-daemon.err"
HOME="${INSTALL_HOME}" CODEX_HOME="${INSTALL_HOME}/.codex" \
    "${MONITOR_BIN}" monitor-logs --install
assert_mode 700 "${INSTALL_HOME}/.codex/log"
assert_mode 700 "${INSTALL_HOME}/.codex/log/archive"
assert_mode 700 "${INSTALL_HOME}/.codex/recovery-runs"
assert_mode 600 "${INSTALL_HOME}/.codex/account-switcher-daemon.log"
assert_mode 600 "${INSTALL_HOME}/.codex/account-switcher-daemon.err"

MISSING_HOME="${TEMP_ROOT}/missing-home"
/bin/mkdir -p "${MISSING_HOME}"
HOME="${MISSING_HOME}" CODEX_HOME="${MISSING_HOME}/.codex" \
    "${MONITOR_BIN}" monitor-logs --install
assert_mode 700 "${MISSING_HOME}/.codex/log"
assert_mode 700 "${MISSING_HOME}/.codex/log/archive"
assert_mode 600 "${MISSING_HOME}/.codex/log/switcher.log"
assert_mode 600 "${MISSING_HOME}/.codex/log/.monitor-log-lifecycle.lock"
assert_absent "${MISSING_HOME}/.codex/recovery-runs"
assert_mode 600 "${MISSING_HOME}/.codex/account-switcher-daemon.log"
assert_mode 600 "${MISSING_HOME}/.codex/account-switcher-daemon.err"

SYMLINK_HOME="${TEMP_ROOT}/symlink-home"
/bin/mkdir -p "${SYMLINK_HOME}/.codex" "${TEMP_ROOT}/outside"
/bin/ln -s "${TEMP_ROOT}/outside" "${SYMLINK_HOME}/.codex/log"
if HOME="${SYMLINK_HOME}" CODEX_HOME="${SYMLINK_HOME}/.codex" \
    "${MONITOR_BIN}" monitor-logs --install; then
    fail 'installer accepted a symlinked log directory'
fi

# Run the uninstaller copy entirely against a fake HOME. Hardcoded system
# integration commands are redirected in the copy so the test cannot unload or
# alter the real user's launch agents or login items.
FAKE_HOME="${TEMP_ROOT}/uninstall-home"
FAKE_ROOT="${TEMP_ROOT}/system"
FAKE_BIN="${TEMP_ROOT}/bin"
/bin/mkdir -p \
    "${FAKE_HOME}/.codex/log/archive" \
    "${FAKE_HOME}/.codex/recovery-runs/nested" \
    "${FAKE_HOME}/.codex/ipc" \
    "${FAKE_HOME}/.codex/app-server-daemon" \
    "${FAKE_HOME}/.codex/sessions" \
    "${FAKE_HOME}/.codex/thread-writer-locks" \
    "${FAKE_ROOT}/Applications/Codex Monitor.app" \
    "${FAKE_BIN}" \
    "${TEMP_ROOT}/tmp"
install_fake_helper "${FAKE_HOME}"
FAKE_CODEX_HOME="$(cd "${FAKE_HOME}/.codex" && /bin/pwd -P)"
/usr/bin/printf 'switcher\n' > "${FAKE_HOME}/.codex/log/switcher.log"
/usr/bin/printf 'lifecycle lock\n' > "${FAKE_HOME}/.codex/log/.monitor-log-lifecycle.lock"
/usr/bin/printf 'switcher archive\n' > "${FAKE_HOME}/.codex/log/archive/switcher-20260920-000000.log.br"
/usr/bin/printf 'daemon archive\n' > "${FAKE_HOME}/.codex/log/archive/account-switcher-daemon-20260920-000000.log.br"
/usr/bin/printf 'daemon error archive\n' > "${FAKE_HOME}/.codex/log/archive/account-switcher-daemon-err-20260920-000000.log.br"
/usr/bin/printf 'malformed archive\n' > "${FAKE_HOME}/.codex/log/archive/switcher-20260920-00000x.log.br"
/usr/bin/printf 'foreign archive\n' > "${FAKE_HOME}/.codex/log/archive/foreign.log.br"
/usr/bin/printf 'keep\n' > "${FAKE_HOME}/.codex/log/archive/keep-unrelated.txt"
/usr/bin/printf 'stdout\n' > "${FAKE_HOME}/.codex/account-switcher-daemon.log"
/usr/bin/printf 'stderr\n' > "${FAKE_HOME}/.codex/account-switcher-daemon.err"
/usr/bin/printf 'temporary\n' > "${FAKE_HOME}/.codex/auth.temporary.tmp.json"
/usr/bin/printf 'redaction temporary\n' > "${FAKE_HOME}/.codex/.redact-1-1.tmp"
/usr/bin/printf 'redaction temporary\n' > "${FAKE_HOME}/.codex/log/.redact-1-2.tmp"
/usr/bin/printf 'redaction temporary\n' > "${FAKE_HOME}/.codex/log/archive/.redact-1-3.tmp"
/usr/bin/printf 'foreign temporary\n' > "${FAKE_HOME}/.codex/.redact-foreign.tmp"
/usr/bin/printf 'unknown recovery\n' > "${FAKE_HOME}/.codex/recovery-runs/nested/unknown-state.bin"
/usr/bin/printf 'auth sentinel\n' > "${FAKE_HOME}/.codex/auth.json"
/usr/bin/printf 'accounts sentinel\n' > "${FAKE_HOME}/.codex/accounts.json"
/usr/bin/printf 'manual reset sentinel\n' > "${FAKE_HOME}/.codex/manual-reset-state.json"
/usr/bin/printf 'desktop session sentinel\n' > "${FAKE_HOME}/.codex/desktop-app-session.json"
/usr/bin/printf 'distribution journal sentinel\n' > "${FAKE_HOME}/.codex/distribution-journal.json"
/usr/bin/printf 'direct switch journal sentinel\n' > "${FAKE_HOME}/.codex/direct-switch-journal.json"
/usr/bin/printf 'distribution staging sentinel\n' > "${FAKE_HOME}/.codex/distribution-journal.123.0123456789abcdef.tmp"
/usr/bin/printf 'direct staging sentinel\n' > "${FAKE_HOME}/.codex/direct-switch-journal.123.0123456789abcdef.tmp"
/usr/bin/printf 'desktop staging sentinel\n' > "${FAKE_HOME}/.codex/desktop-app-session.123.0123456789abcdef0123456789abcdef.tmp"
/usr/bin/printf 'foreign staging sentinel\n' > "${FAKE_HOME}/.codex/distribution-journal.abc.0123456789abcdef.tmp"
# ManualResetAttemptStore stages `manual-reset-state.<pid>.<16 hex>.tmp.json`;
# ActiveAuthCompareWriteService stages `auth.json.<pid>.<16 hex>.tmp`. For both
# names, each look-alike differs from a valid file in exactly one validated
# property: decimal pid, nonce length, lowercase-hex nonce, or 0600 mode. A
# valid-name directory and a valid-name symlink (link mode 0600) must also
# survive. A file owned by another uid needs root to create and is not tested.
# Do not use uppercase hex look-alikes: the default APFS volume is
# case-insensitive, so they alias the valid fixture.
/usr/bin/printf 'manual staging sentinel\n' > "${FAKE_HOME}/.codex/manual-reset-state.123.0123456789abcdef.tmp.json"
/usr/bin/printf 'auth staging sentinel\n' > "${FAKE_HOME}/.codex/auth.json.123.0123456789abcdef.tmp"
STAGING_LOOKALIKES_0600=(
    manual-reset-state.abc.0123456789abcdef.tmp.json
    manual-reset-state.123.0123456789abcde.tmp.json
    manual-reset-state.123.0123456789abcdeg.tmp.json
    auth.json.abc.0123456789abcdef.tmp
    auth.json.123.0123456789abcde.tmp
    auth.json.123.0123456789abcdeg.tmp
)
STAGING_LOOKALIKES_0644=(
    manual-reset-state.124.0123456789abcdef.tmp.json
    auth.json.124.0123456789abcdef.tmp
)
for name in "${STAGING_LOOKALIKES_0600[@]}" "${STAGING_LOOKALIKES_0644[@]}"; do
    /usr/bin/printf 'look-alike %s\n' "${name}" > "${FAKE_HOME}/.codex/${name}"
done
STAGING_DIRECTORY="${FAKE_HOME}/.codex/auth.json.125.0123456789abcdef.tmp"
STAGING_SYMLINK="${FAKE_HOME}/.codex/manual-reset-state.126.0123456789abcdef.tmp.json"
STAGING_SYMLINK_TARGET="${TEMP_ROOT}/staging-symlink-target"
/bin/mkdir -m 600 "${STAGING_DIRECTORY}"
/usr/bin/printf 'symlink target sentinel\n' > "${STAGING_SYMLINK_TARGET}"
/bin/ln -s "${STAGING_SYMLINK_TARGET}" "${STAGING_SYMLINK}"
/bin/chmod -h 600 "${STAGING_SYMLINK}"
/usr/bin/printf 'sqlite sentinel\n' > "${FAKE_HOME}/.codex/state_5.sqlite"
/usr/bin/printf 'wal sentinel\n' > "${FAKE_HOME}/.codex/state_5.sqlite-wal"
/usr/bin/printf 'shm sentinel\n' > "${FAKE_HOME}/.codex/state_5.sqlite-shm"
/usr/bin/printf 'ipc sentinel\n' > "${FAKE_HOME}/.codex/ipc/socket-placeholder"
/usr/bin/printf 'app-server sentinel\n' > "${FAKE_HOME}/.codex/app-server-daemon/owner"
/usr/bin/printf 'session sentinel\n' > "${FAKE_HOME}/.codex/sessions/transcript"
/usr/bin/printf 'lock sentinel\n' > "${FAKE_HOME}/.codex/thread-writer-locks/thread.lock"
/bin/chmod 600 \
    "${FAKE_HOME}/.codex/auth.json" \
    "${FAKE_HOME}/.codex/accounts.json" \
    "${FAKE_HOME}/.codex/manual-reset-state.json" \
    "${FAKE_HOME}/.codex/desktop-app-session.json" \
    "${FAKE_HOME}/.codex/distribution-journal.json" \
    "${FAKE_HOME}/.codex/direct-switch-journal.json" \
    "${FAKE_HOME}/.codex/distribution-journal.123.0123456789abcdef.tmp" \
    "${FAKE_HOME}/.codex/direct-switch-journal.123.0123456789abcdef.tmp" \
    "${FAKE_HOME}/.codex/desktop-app-session.123.0123456789abcdef0123456789abcdef.tmp" \
    "${FAKE_HOME}/.codex/manual-reset-state.123.0123456789abcdef.tmp.json" \
    "${FAKE_HOME}/.codex/auth.json.123.0123456789abcdef.tmp" \
    "${FAKE_HOME}/.codex/state_5.sqlite" \
    "${FAKE_HOME}/.codex/state_5.sqlite-wal" \
    "${FAKE_HOME}/.codex/state_5.sqlite-shm"
/usr/bin/printf '#!/bin/sh\nexit 0\n' > "${FAKE_BIN}/launchctl"
/usr/bin/printf '#!/bin/sh\nexit 0\n' > "${FAKE_BIN}/osascript"
/usr/bin/printf '#!/bin/sh\nexit 0\n' > "${FAKE_BIN}/defaults"
/usr/bin/printf '#!/bin/sh\nexit 0\n' > "${FAKE_BIN}/lsregister"
# `getconf DARWIN_USER_TEMP_DIR` names the per-user temporary directory that
# `mktemp -t` uses. The fake prints GETCONF_OUTPUT, or fails when it is absent.
GETCONF_OUTPUT="${TEMP_ROOT}/getconf-output"
FAKE_DARWIN_TMP="${TEMP_ROOT}/darwin-user-tmp"
/bin/mkdir -p "${FAKE_DARWIN_TMP}"
/usr/bin/printf '%s/\n' "${FAKE_DARWIN_TMP}" > "${GETCONF_OUTPUT}"
/bin/cat > "${FAKE_BIN}/getconf" <<EOF
#!/bin/sh
[ "\$#" -eq 1 ] && [ "\$1" = DARWIN_USER_TEMP_DIR ] && [ -f '${GETCONF_OUTPUT}' ] || exit 1
/bin/cat '${GETCONF_OUTPUT}'
EOF
# The uninstaller stops this user's processes whose command line names an
# installed path. The /bin/ps fake lists PS_TABLE ("<pid> <uid> <command>"
# lines; empty means no processes). The /bin/kill fake reports a PID alive
# while ALIVE lists it, logs TERM and KILL, and ends the process on TERM
# unless STUBBORN lists it.
PS_TABLE="${TEMP_ROOT}/ps-table"
ALIVE="${TEMP_ROOT}/alive"
STUBBORN="${TEMP_ROOT}/stubborn"
KILL_LOG="${TEMP_ROOT}/kill.log"
: > "${PS_TABLE}"
: > "${ALIVE}"
: > "${STUBBORN}"
/bin/cat > "${FAKE_BIN}/ps" <<EOF
#!/bin/bash
case "\$*" in
    '-axo pid=,uid=,command=') /bin/cat '${PS_TABLE}' ;;
    '-p '*' -o command=')
        /usr/bin/awk -v pid="\$2" '\$1 == pid { sub(/^[0-9]+ [0-9]+ /, ""); print }' '${PS_TABLE}' ;;
    *) echo "unexpected ps \$*" >&2; exit 2 ;;
esac
EOF
/bin/cat > "${FAKE_BIN}/kill" <<EOF
#!/bin/bash
case "\$1" in
    -0) /usr/bin/grep -F -x -- "\$2" '${ALIVE}' >/dev/null ;;
    -TERM|-KILL)
        /usr/bin/printf '%s %s\n' "\$1" "\$2" >> '${KILL_LOG}'
        if [ "\$1" = -KILL ] || ! /usr/bin/grep -F -x -- "\$2" '${STUBBORN}' >/dev/null; then
            /usr/bin/grep -v -F -x -- "\$2" '${ALIVE}' > '${ALIVE}.next' || true
            /bin/mv -f '${ALIVE}.next' '${ALIVE}'
        fi
        ;;
    *) echo "unexpected kill \$*" >&2; exit 2 ;;
esac
EOF
# Where an installer older than the per-user temporary directory lock left
# its lock when TMPDIR was unset.
FAKE_SHARED_TMP="${TEMP_ROOT}/shared-tmp"
/bin/mkdir -p "${FAKE_SHARED_TMP}"
for name in "${STAGING_LOOKALIKES_0600[@]}"; do /bin/chmod 600 "${FAKE_HOME}/.codex/${name}"; done
for name in "${STAGING_LOOKALIKES_0644[@]}"; do /bin/chmod 644 "${FAKE_HOME}/.codex/${name}"; done
[ "$(/usr/bin/stat -f '%Lp' "${STAGING_SYMLINK}")" = 600 ] || fail 'symlink look-alike mode setup failed'
/bin/chmod 755 "${FAKE_BIN}/launchctl" "${FAKE_BIN}/osascript" "${FAKE_BIN}/defaults" \
    "${FAKE_BIN}/lsregister" "${FAKE_BIN}/getconf" "${FAKE_BIN}/ps" "${FAKE_BIN}/kill"

# Installed helpers and a system install's ~/Applications symlink.
/bin/mkdir -p "${FAKE_HOME}/Applications/Codex Notifier.app/Contents/MacOS"
/bin/ln -s "${FAKE_ROOT}/Applications/Codex Monitor.app" "${FAKE_HOME}/Applications/Codex Monitor.app"
INSTALLED_HELPERS=(codex-ui-resume codex-recovery-banner codex-window-restore)
for name in "${INSTALLED_HELPERS[@]}"; do
    /usr/bin/printf 'helper %s\n' "${name}" > "${FAKE_HOME}/.local/bin/${name}"
    /bin/chmod 755 "${FAKE_HOME}/.local/bin/${name}"
done

# ------------------------------------------------------------------------------
# Leftovers of a killed install (see installer_temps in scripts/uninstall.sh).
# Each look-alike differs from a valid leftover in exactly one validated
# property: prefix, suffix length, mktemp alphabet, type, mode, a symlink, or
# the content of a staging root. A leftover owned by another uid needs root to
# create and is not tested. Names are distinct ignoring case, because the
# default APFS volume is case-insensitive.
# ------------------------------------------------------------------------------
OUTSIDE="${TEMP_ROOT}/outside-targets"
/bin/mkdir -p "${OUTSIDE}/dir-target" "${OUTSIDE}/bundle-target.app" "${OUTSIDE}/empty-root-target"
/bin/chmod 700 "${OUTSIDE}/dir-target" "${OUTSIDE}/empty-root-target"
/usr/bin/printf 'outside file sentinel\n' > "${OUTSIDE}/file-target"
/usr/bin/printf 'outside dir sentinel\n' > "${OUTSIDE}/dir-target/sentinel"
/usr/bin/printf 'outside bundle sentinel\n' > "${OUTSIDE}/bundle-target.app/sentinel"

make_file() {
    local mode="$1"
    local path="$2"
    /usr/bin/printf 'fixture %s\n' "${path##*/}" > "${path}"
    /bin/chmod "${mode}" "${path}"
}

make_staging_root() {
    local mode="$1"
    local path="$2"
    shift 2
    /bin/mkdir -p "${path}"
    local entry
    for entry in "$@"; do
        case "${entry}" in
            */) /bin/mkdir -p "${path}/${entry}Contents"
                /usr/bin/printf 'bundle\n' > "${path}/${entry}Contents/Info.plist" ;;
            *) /usr/bin/printf 'entry\n' > "${path}/${entry}" ;;
        esac
    done
    /bin/chmod "${mode}" "${path}"
}

LOCAL_BIN_FIXTURE="${FAKE_HOME}/.local/bin"
USER_APPS="${FAKE_HOME}/Applications"
SYSTEM_APPS="${FAKE_ROOT}/Applications"
VALID_INSTALLER_TEMPS=(
    "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dE9"
    "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Zz09aQ"
    "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Zz09aQ.cstemp"
    "${SYSTEM_APPS}/.codex-monitor-install.Q1w2E3"
    "${SYSTEM_APPS}/.codex-monitor-backup.R4t5Y6"
    "${USER_APPS}/.codex-monitor-install.U7i8O9"
    "${USER_APPS}/.codex-monitor-backup.P0a1S2"
    "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D4e5"
    "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.Zz9Yy8Xx7W"
    "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.m13Cln8x"
)
# Killed before `chmod 755`, after it, and during codesign's temporary copy.
make_file 600 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dE9"
make_file 755 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Zz09aQ"
make_file 755 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Zz09aQ.cstemp"
# A staged new bundle, a backup holding a prior bundle (as after a kill
# between the two renames, or a failed rollback; the uninstaller does not
# depend on whether the installed app is present), and an empty stage root
# and backup root.
make_staging_root 700 "${SYSTEM_APPS}/.codex-monitor-install.Q1w2E3" 'Codex Monitor.app/'
make_staging_root 700 "${SYSTEM_APPS}/.codex-monitor-backup.R4t5Y6" 'Codex Monitor.app/'
make_staging_root 700 "${USER_APPS}/.codex-monitor-install.U7i8O9"
make_staging_root 700 "${USER_APPS}/.codex-monitor-backup.P0a1S2"
# A remote-install clone with sources, and one killed right after mktemp.
# `mktemp -t` appends ten characters from macOS 14 and eight on macOS 13.
make_staging_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D4e5" 'Cargo.toml' '.git/'
make_staging_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.Zz9Yy8Xx7W"
make_staging_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.m13Cln8x" 'Cargo.toml'

LOOKALIKE_INSTALLER_TEMPS=()
lookalike_file() {
    make_file "$1" "$2"
    LOOKALIKE_INSTALLER_TEMPS+=("$2")
}
lookalike_root() {
    make_staging_root "$@"
    LOOKALIKE_INSTALLER_TEMPS+=("$2")
}
# A symlink look-alike gets the link mode a valid leftover would have, so only
# the symlink check can reject it.
lookalike_symlink() {
    /bin/ln -s "$1" "$2"
    [ -z "${3:-}" ] || /bin/chmod -h "$3" "$2"
    LOOKALIKE_INSTALLER_TEMPS+=("$2")
}
lookalike_file 600 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dE"
lookalike_file 600 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dE9x"
lookalike_file 600 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3d_9"
lookalike_file 600 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dé9"
lookalike_file 600 "${LOCAL_BIN_FIXTURE}/codex-mon.install.Ab3dE9"
lookalike_file 644 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Mo644d"
lookalike_file 644 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ce644x.cstemp"
lookalike_file 600 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ce600x.cstemp"
lookalike_file 755 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dE9.cstem"
lookalike_file 755 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.Ab3dE.cstemp"
lookalike_root 755 "${LOCAL_BIN_FIXTURE}/.codex-mon.install.DirDir"
lookalike_symlink "${OUTSIDE}/file-target" "${LOCAL_BIN_FIXTURE}/.codex-mon.install.LnkLnk" 600
lookalike_symlink "${OUTSIDE}/file-target" "${LOCAL_BIN_FIXTURE}/.codex-mon.install.LnkCst.cstemp" 755
lookalike_root 700 "${SYSTEM_APPS}/.codex-monitor-install.Q1w2E"
lookalike_root 700 "${SYSTEM_APPS}/.codex-monitor-backup.R4t5Y6Z"
lookalike_root 700 "${SYSTEM_APPS}/.codex-monitor-backup.R4t5-6"
lookalike_root 700 "${SYSTEM_APPS}/codex-monitor-install.Q1w2E3"
lookalike_root 700 "${SYSTEM_APPS}/.codex-monitor-installer.Q1w2E3"
lookalike_root 755 "${SYSTEM_APPS}/.codex-monitor-install.Md0755" 'Codex Monitor.app/'
lookalike_file 700 "${SYSTEM_APPS}/.codex-monitor-install.FiLe01"
lookalike_symlink "${OUTSIDE}/empty-root-target" "${SYSTEM_APPS}/.codex-monitor-backup.SyMl01" 700
lookalike_root 700 "${USER_APPS}/.codex-monitor-backup.XtRa01" 'Codex Monitor.app/' 'notes.txt'
lookalike_root 700 "${USER_APPS}/.codex-monitor-backup.OtHr01" 'Other.app/'
lookalike_root 700 "${USER_APPS}/.codex-monitor-install.HiDn01" '.hidden'
lookalike_root 700 "${USER_APPS}/.codex-monitor-install.FlBd01" 'Codex Monitor.app'
/bin/mkdir -m 700 "${USER_APPS}/.codex-monitor-install.BlNk01"
lookalike_symlink "${OUTSIDE}/bundle-target.app" "${USER_APPS}/.codex-monitor-install.BlNk01/Codex Monitor.app"
LOOKALIKE_INSTALLER_TEMPS+=("${USER_APPS}/.codex-monitor-install.BlNk01")
lookalike_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D"
lookalike_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D4e"
lookalike_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D4e5f"
lookalike_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.a1B2c3D4_5"
lookalike_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-abc123.a1B2c3D4e5"
lookalike_root 755 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.Mode755abc"
lookalike_file 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.FileFile01"
lookalike_symlink "${OUTSIDE}/dir-target" "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.SymLink001" 700
# The per-user temporary directory is the only place `mktemp -t` clones go;
# a clone-shaped directory in TMPDIR is not the installer's.
lookalike_root 700 "${TEMP_ROOT}/tmp/codex-mon-install-XXXXXX.a1B2c3D4e5"

# Private copies an interrupted uninstall leaves while it edits a shell or
# Codex config. The copy's mode follows the edited file, so only owner, type,
# and name are checked.
VALID_UNINSTALLER_TEMPS=(
    "${FAKE_HOME}/.zshrc.codex-monitor-uninstall.K3l4M5"
    "${FAKE_HOME}/.bash_profile.codex-monitor-uninstall.B9n8M7"
    "${FAKE_HOME}/.codex/config.toml.codex-monitor-uninstall.C1v2B3"
)
make_file 600 "${FAKE_HOME}/.zshrc.codex-monitor-uninstall.K3l4M5"
make_file 644 "${FAKE_HOME}/.bash_profile.codex-monitor-uninstall.B9n8M7"
make_file 600 "${FAKE_HOME}/.codex/config.toml.codex-monitor-uninstall.C1v2B3"
# The same kind of copy made with the uninstaller's own mktemp template.
for edited in .zshrc .bash_profile .codex/config.toml; do
    VALID_UNINSTALLER_TEMPS+=("$(/usr/bin/mktemp "${FAKE_HOME}/${edited}${UNINSTALL_COPY_SUFFIX}")")
done
lookalike_file 600 "${FAKE_HOME}/.zshrc.codex-monitor-uninstall.K3l4M"
lookalike_file 600 "${FAKE_HOME}/.zshrc.codex-monitor-uninstal.K3l4M5"
lookalike_file 600 "${FAKE_HOME}/.profile.codex-monitor-uninstall.K3l4M5"
lookalike_file 600 "${FAKE_HOME}/.codex/.zshrc.codex-monitor-uninstall.Q9w8E7"
lookalike_root 700 "${FAKE_HOME}/.zshrc.codex-monitor-uninstall.D1rD1r"
lookalike_symlink "${OUTSIDE}/file-target" "${FAKE_HOME}/.bash_profile.codex-monitor-uninstall.L1nK01"

# The fakes' paths are spliced into sed programs below, which only works for
# plain path characters.
case "${TEMP_ROOT}" in
    *[!A-Za-z0-9/._+-]*) fail "TMPDIR must be a path of letters, digits, /, ., _, +, and -: ${TEMP_ROOT}" ;;
esac
UNINSTALL_COPY="${TEMP_ROOT}/uninstall.sh"
# Rewrite only absolute system paths (after a quote or a space); the
# `${USER_HOME}/Applications` paths already point into the fake home.
/usr/bin/sed \
    -e "s#\\([\" ]\\)/Applications\\([/\" ]\\)#\\1${FAKE_ROOT}/Applications\\2#g" \
    -e "s#\"/usr/local/bin/#\"${FAKE_ROOT}/usr/local/bin/#g" \
    -e "s|/bin/launchctl|${FAKE_BIN}/launchctl|g" \
    -e "s|/usr/bin/osascript|${FAKE_BIN}/osascript|g" \
    -e "s|/usr/bin/defaults|${FAKE_BIN}/defaults|g" \
    -e "s|/usr/bin/getconf|${FAKE_BIN}/getconf|g" \
    -e "s|/bin/ps |${FAKE_BIN}/ps |g" \
    -e "s|/bin/kill |${FAKE_BIN}/kill |g" \
    -e "s|\${TMPDIR:-/tmp}|\${TMPDIR:-${FAKE_SHARED_TMP}}|g" \
    -e "s|\"/tmp\"|\"${FAKE_SHARED_TMP}\"|g" \
    -e "s|/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister|${FAKE_BIN}/lsregister|g" \
    "${UNINSTALL_SCRIPT}" > "${UNINSTALL_COPY}"
/bin/chmod 755 "${UNINSTALL_COPY}"
# Fail if a command line of the copy still names a real system path the test
# must not touch, or a process, launchd, or temporary-directory command by
# another spelling that the rewrite missed; comment lines never run. The
# copies below differ only in the lock tools.
SYSTEM_PATH_PATTERNS=(
    -e '/Applications' -e '/usr/local/bin' -e '/usr/bin/getconf' -e '/bin/launchctl'
    -e '/usr/bin/osascript' -e '/usr/bin/defaults' -e '/Support/lsregister' -e '/bin/ps'
    -e '/bin/kill' -e '/tmp'
)
SYSTEM_COMMAND='(^|[^[:alnum:]_.])(ps|kill|pkill|killall|pgrep|lsappinfo|launchctl|osascript|defaults|getconf)([^[:alnum:]_.-]|$)|(^|[^[:alnum:]_.-])open[[:space:]]'
SOURCE_COMMAND='(^|[;&|({[:space:]])(source|[.])[[:space:]]'
unrewritten_commands() {
    /usr/bin/sed -e "s|${FAKE_BIN}/[A-Za-z0-9_-]*||g" -e "s|${FAKE_ROOT}/||g" -e "s|${TEMP_ROOT}/||g" \
        -e 's|}/Applications||g' "$1" |
        /usr/bin/grep -n -E "${SYSTEM_PATH_PATTERNS[@]}" -e "${SYSTEM_COMMAND}" -e "${SOURCE_COMMAND}" |
        /usr/bin/grep -v -E '^[0-9]+:[[:space:]]*#' || true
}
# The check must reject each of these lines and accept fakes and comments.
GUARD_PROBE="${TEMP_ROOT}/uninstall-guard-probe.sh"
for line in 'kill -TERM "$pid"' '/bin/kill -0 1' 'x="$(ps -axo pid=)"' '/usr/bin/pkill CodexMonitor' \
    'launchctl bootout gui/501/x' 'command -v osascript' '/usr/bin/defaults delete x' \
    'dir="$(getconf DARWIN_USER_TEMP_DIR)"' '/usr/bin/open -a x' 'TMP_ROOT="${TMPDIR:-/tmp}"' \
    'add_install_lock_dir "/tmp"' 'rm -f /Applications/x' 'source "${x}"' '    . ./x.sh'; do
    /usr/bin/printf '%s\n' "${line}" > "${GUARD_PROBE}"
    [ -n "$(unrewritten_commands "${GUARD_PROBE}")" ] || fail "uninstaller copy guard missed: ${line}"
done
/usr/bin/printf '%s\n' "\"${FAKE_BIN}/kill\" -TERM 1" "${FAKE_BIN}/ps -axo pid=" '# kill ps /tmp' \
    "add_install_lock_dir \"${FAKE_SHARED_TMP}\"" 'open(my $lock, "<&=", $ARGV[0]) or exit 71;' \
    'warn "process $pid changed identity"' > "${GUARD_PROBE}"
[ -z "$(unrewritten_commands "${GUARD_PROBE}")" ] || fail 'uninstaller copy guard rejected a fake or a comment'
UNREWRITTEN_PATHS="$(unrewritten_commands "${UNINSTALL_COPY}")"
[ -z "${UNREWRITTEN_PATHS}" ] ||
    fail "uninstaller copy still reaches real system paths or commands:
${UNREWRITTEN_PATHS}"

DRY_RUN_OUTPUT="${TEMP_ROOT}/dry-run.txt"
HOME="${FAKE_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --dry-run > "${DRY_RUN_OUTPUT}"
/usr/bin/grep -F 'switcher.log' "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/log/switcher.log" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/log/.monitor-log-lifecycle.lock" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F 'switcher-20260920-000000.log.br' "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F 'account-switcher-daemon-20260920-000000.log.br' "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F 'account-switcher-daemon-err-20260920-000000.log.br' "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F 'switcher-20260920-00000x.log.br' "${DRY_RUN_OUTPUT}" >/dev/null &&
    fail 'dry-run listed a malformed Monitor archive'
/usr/bin/grep -F 'foreign.log.br' "${DRY_RUN_OUTPUT}" >/dev/null &&
    fail 'dry-run listed a foreign Brotli archive'
/usr/bin/grep -F 'preserve foreign Brotli archives' "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/account-switcher-daemon.log" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/account-switcher-daemon.err" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/recovery-runs" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/manual-reset-state.json" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/desktop-app-session.json" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/distribution-journal.json" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/direct-switch-journal.json" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/distribution-journal.123.0123456789abcdef.tmp" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/direct-switch-journal.123.0123456789abcdef.tmp" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/desktop-app-session.123.0123456789abcdef0123456789abcdef.tmp" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F 'distribution-journal.abc.0123456789abcdef.tmp' "${DRY_RUN_OUTPUT}" >/dev/null &&
    fail 'dry-run listed a foreign staging file'
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/manual-reset-state.123.0123456789abcdef.tmp.json" "${DRY_RUN_OUTPUT}" >/dev/null ||
    fail 'dry-run omitted manual reset staging'
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/auth.json.123.0123456789abcdef.tmp" "${DRY_RUN_OUTPUT}" >/dev/null ||
    fail 'dry-run omitted credential compare-write staging'
for foreign in "${STAGING_LOOKALIKES_0600[@]}" "${STAGING_LOOKALIKES_0644[@]}" \
    "${STAGING_DIRECTORY##*/}" "${STAGING_SYMLINK##*/}"; do
    /usr/bin/grep -F "${foreign}" "${DRY_RUN_OUTPUT}" >/dev/null &&
        fail "dry-run listed foreign staging look-alike ${foreign}"
done
assert_exists "${FAKE_HOME}/.codex/manual-reset-state.123.0123456789abcdef.tmp.json"
assert_exists "${FAKE_HOME}/.codex/auth.json.123.0123456789abcdef.tmp"
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/auth.temporary.tmp.json" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/.redact-1-1.tmp" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/log/.redact-1-2.tmp" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F "  remove ${FAKE_CODEX_HOME}/log/archive/.redact-1-3.tmp" "${DRY_RUN_OUTPUT}" >/dev/null
/usr/bin/grep -F '.redact-foreign.tmp' "${DRY_RUN_OUTPUT}" >/dev/null &&
    fail 'dry-run listed a foreign redaction-style file'
assert_exists "${FAKE_HOME}/.codex/log/switcher.log"
assert_exists "${FAKE_HOME}/.codex/recovery-runs/nested/unknown-state.bin"
for path in "${VALID_INSTALLER_TEMPS[@]}"; do
    /usr/bin/grep -F -x "  remove ${path}" "${DRY_RUN_OUTPUT}" >/dev/null ||
        fail "dry-run omitted interrupted install leftover ${path}"
    assert_exists "${path}"
done
for path in "${VALID_UNINSTALLER_TEMPS[@]}"; do
    /usr/bin/grep -F -x "  remove ${path}" "${DRY_RUN_OUTPUT}" >/dev/null ||
        fail "dry-run omitted interrupted uninstall copy ${path}"
    assert_exists "${path}"
done
for path in "${LOOKALIKE_INSTALLER_TEMPS[@]}"; do
    if output_names_path "${DRY_RUN_OUTPUT}" "${path}"; then
        fail "dry-run listed look-alike ${path}"
    fi
done
/usr/bin/grep -F -x "  remove ${FAKE_HOME}/Applications/Codex Monitor.app" "${DRY_RUN_OUTPUT}" >/dev/null ||
    fail 'dry-run omitted the ~/Applications Monitor symlink'
/usr/bin/grep -F 'preserve' "${DRY_RUN_OUTPUT}" | /usr/bin/grep -F -e 'install lock' -e 'temporary directory' >/dev/null &&
    fail 'dry-run preserved installer leftovers although no installer was running'

HOME="${FAKE_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes >/dev/null
assert_absent "${FAKE_HOME}/.codex/log/switcher.log"
assert_absent "${FAKE_HOME}/.codex/log/.monitor-log-lifecycle.lock"
assert_absent "${FAKE_HOME}/.codex/log/archive/switcher-20260920-000000.log.br"
assert_exists "${FAKE_HOME}/.codex/log/archive/keep-unrelated.txt"
assert_absent "${FAKE_HOME}/.codex/account-switcher-daemon.log"
assert_absent "${FAKE_HOME}/.codex/account-switcher-daemon.err"
assert_absent "${FAKE_HOME}/.codex/recovery-runs"
assert_absent "${FAKE_HOME}/.codex/manual-reset-state.json"
assert_absent "${FAKE_HOME}/.codex/desktop-app-session.json"
assert_absent "${FAKE_HOME}/.codex/distribution-journal.json"
assert_absent "${FAKE_HOME}/.codex/direct-switch-journal.json"
assert_absent "${FAKE_HOME}/.codex/distribution-journal.123.0123456789abcdef.tmp"
assert_absent "${FAKE_HOME}/.codex/direct-switch-journal.123.0123456789abcdef.tmp"
assert_absent "${FAKE_HOME}/.codex/desktop-app-session.123.0123456789abcdef0123456789abcdef.tmp"
assert_exists "${FAKE_HOME}/.codex/distribution-journal.abc.0123456789abcdef.tmp"
assert_absent "${FAKE_HOME}/.codex/manual-reset-state.123.0123456789abcdef.tmp.json"
assert_absent "${FAKE_HOME}/.codex/auth.json.123.0123456789abcdef.tmp"
for name in "${STAGING_LOOKALIKES_0600[@]}" "${STAGING_LOOKALIKES_0644[@]}"; do
    assert_content "look-alike ${name}" "${FAKE_HOME}/.codex/${name}"
done
[ -d "${STAGING_DIRECTORY}" ] && [ ! -L "${STAGING_DIRECTORY}" ] ||
    fail 'uninstall removed a valid-name staging directory'
[ -L "${STAGING_SYMLINK}" ] || fail 'uninstall removed a valid-name staging symlink'
assert_content 'symlink target sentinel' "${STAGING_SYMLINK_TARGET}"
assert_absent "${FAKE_HOME}/.codex/auth.temporary.tmp.json"
assert_absent "${FAKE_HOME}/.codex/.redact-1-1.tmp"
assert_absent "${FAKE_HOME}/.codex/log/.redact-1-2.tmp"
assert_absent "${FAKE_HOME}/.codex/log/archive/.redact-1-3.tmp"
assert_exists "${FAKE_HOME}/.codex/.redact-foreign.tmp"
assert_absent "${FAKE_ROOT}/Applications/Codex Monitor.app"
assert_absent "${FAKE_HOME}/.codex/log/archive/switcher-20260920-000000.log.br"
assert_absent "${FAKE_HOME}/.codex/log/archive/account-switcher-daemon-20260920-000000.log.br"
assert_absent "${FAKE_HOME}/.codex/log/archive/account-switcher-daemon-err-20260920-000000.log.br"
assert_exists "${FAKE_HOME}/.codex/log/archive/foreign.log.br"
assert_exists "${FAKE_HOME}/.codex/log/archive/switcher-20260920-00000x.log.br"
assert_exists "${FAKE_HOME}/.codex/log/archive/keep-unrelated.txt"
for name in "${INSTALLED_HELPERS[@]}"; do
    assert_absent "${FAKE_HOME}/.local/bin/${name}"
done
assert_absent "${FAKE_HOME}/Applications/Codex Monitor.app"
assert_absent "${FAKE_HOME}/Applications/Codex Notifier.app"
for path in "${VALID_INSTALLER_TEMPS[@]}" "${VALID_UNINSTALLER_TEMPS[@]}"; do
    assert_absent "${path}"
done
for path in "${LOOKALIKE_INSTALLER_TEMPS[@]}"; do
    [ -e "${path}" ] || [ -L "${path}" ] || fail "uninstall removed look-alike ${path}"
    if [ -f "${path}" ] && [ ! -L "${path}" ]; then
        assert_content "fixture ${path##*/}" "${path}"
    fi
done
for path in \
    "${USER_APPS}/.codex-monitor-backup.XtRa01/Codex Monitor.app/Contents/Info.plist" \
    "${USER_APPS}/.codex-monitor-backup.XtRa01/notes.txt" \
    "${USER_APPS}/.codex-monitor-backup.OtHr01/Other.app/Contents/Info.plist" \
    "${USER_APPS}/.codex-monitor-install.HiDn01/.hidden" \
    "${SYSTEM_APPS}/.codex-monitor-install.Md0755/Codex Monitor.app/Contents/Info.plist"; do
    assert_exists "${path}"
done
assert_content 'outside file sentinel' "${OUTSIDE}/file-target"
assert_content 'outside dir sentinel' "${OUTSIDE}/dir-target/sentinel"
assert_content 'outside bundle sentinel' "${OUTSIDE}/bundle-target.app/sentinel"
[ -d "${OUTSIDE}/empty-root-target" ] || fail 'uninstall removed a symlinked staging root target'

for protected in \
    "${FAKE_HOME}/.codex/auth.json" \
    "${FAKE_HOME}/.codex/accounts.json" \
    "${FAKE_HOME}/.codex/state_5.sqlite" \
    "${FAKE_HOME}/.codex/state_5.sqlite-wal" \
    "${FAKE_HOME}/.codex/state_5.sqlite-shm" \
    "${FAKE_HOME}/.codex/ipc/socket-placeholder" \
    "${FAKE_HOME}/.codex/app-server-daemon/owner" \
    "${FAKE_HOME}/.codex/sessions/transcript" \
    "${FAKE_HOME}/.codex/thread-writer-locks/thread.lock"; do
    assert_exists "${protected}"
done
assert_content 'auth sentinel' "${FAKE_HOME}/.codex/auth.json"
assert_content 'accounts sentinel' "${FAKE_HOME}/.codex/accounts.json"
assert_content 'sqlite sentinel' "${FAKE_HOME}/.codex/state_5.sqlite"
assert_content 'wal sentinel' "${FAKE_HOME}/.codex/state_5.sqlite-wal"
assert_content 'shm sentinel' "${FAKE_HOME}/.codex/state_5.sqlite-shm"
assert_mode 600 "${FAKE_HOME}/.codex/auth.json"
assert_mode 600 "${FAKE_HOME}/.codex/accounts.json"

# ------------------------------------------------------------------------------
# The uninstaller stops only this user's processes whose command line names an
# installed path. One that ignores TERM gets KILL while its command line still
# names that path.
# ------------------------------------------------------------------------------
TEST_UID="$(/usr/bin/id -u)"
PROCESS_HOME="${TEMP_ROOT}/process-home"
/bin/mkdir -p "${PROCESS_HOME}/.codex"
install_fake_helper "${PROCESS_HOME}"
/usr/bin/printf '%s\n' \
    "4101 ${TEST_UID} ${PROCESS_HOME}/.local/bin/codex-mon daemon" \
    "4102 $((TEST_UID + 1)) ${PROCESS_HOME}/.local/bin/codex-mon daemon" \
    "4103 ${TEST_UID} /usr/bin/unrelated --flag" \
    "4104 ${TEST_UID} ${FAKE_ROOT}/Applications/Codex Monitor.app/Contents/MacOS/CodexMonitor" \
    "4105 ${TEST_UID} ${PROCESS_HOME}/.local/bin/codex-ui-resume --banner" > "${PS_TABLE}"
/usr/bin/printf '%s\n' 4101 4102 4103 4104 4105 > "${ALIVE}"
/usr/bin/printf '4105\n' > "${STUBBORN}"
HOME="${PROCESS_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes > "${TEMP_ROOT}/process-output.txt" 2>&1 ||
    fail "uninstall failed while stopping processes: $(/bin/cat "${TEMP_ROOT}/process-output.txt")"
[ "$(/bin/cat "${KILL_LOG}")" = '-TERM 4101
-TERM 4105
-KILL 4105
-TERM 4104' ] || fail "uninstall signalled the wrong processes: $(/bin/cat "${KILL_LOG}")"
: > "${PS_TABLE}"
: > "${ALIVE}"
: > "${STUBBORN}"
/bin/rm -f "${KILL_LOG}"

# ------------------------------------------------------------------------------
# The install lock. install.sh locks codex_monitor_install_<uid>.lock in the
# per-user temporary directory (here FAKE_DARWIN_TMP, through the fake
# getconf) whatever its TMPDIR says; older installers locked the one in
# ${TMPDIR:-/tmp}. While an installer holds a lock, its backup root can hold
# the only copy of the previous app, so a confirmed uninstall stops before
# changing anything and a dry run says so. Leftovers are also kept, as a
# second layer, from an installer that starts after that check, and whenever
# the lock cannot be verified. The uninstaller's own copies do not depend on
# the lock.
# ------------------------------------------------------------------------------
LOCKED_HOME="${TEMP_ROOT}/locked-home"
/bin/mkdir -p "${LOCKED_HOME}/.codex" "${LOCKED_HOME}/Applications"
install_fake_helper "${LOCKED_HOME}"
LOCKED_TEMPS=(
    "${LOCKED_HOME}/.local/bin/.codex-mon.install.L0cK01"
    "${SYSTEM_APPS}/.codex-monitor-backup.L0cK02"
    "${LOCKED_HOME}/Applications/.codex-monitor-install.L0cK03"
    "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.L0cK04abcd"
)
make_file 600 "${LOCKED_HOME}/.local/bin/.codex-mon.install.L0cK01"
make_staging_root 700 "${SYSTEM_APPS}/.codex-monitor-backup.L0cK02" 'Codex Monitor.app/'
make_staging_root 700 "${LOCKED_HOME}/Applications/.codex-monitor-install.L0cK03" 'Codex Monitor.app/'
make_staging_root 700 "${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.L0cK04abcd" 'Cargo.toml'
LOCKED_UNINSTALL_TEMP="${LOCKED_HOME}/.zshrc.codex-monitor-uninstall.L0cK05"
make_file 600 "${LOCKED_UNINSTALL_TEMP}"
LOCK_NAME="codex_monitor_install_${TEST_UID}.lock"
INSTALL_LOCK="${FAKE_DARWIN_TMP}/${LOCK_NAME}"
TMPDIR_LOCK="${TEMP_ROOT}/tmp/${LOCK_NAME}"
SHARED_TMP_LOCK="${FAKE_SHARED_TMP}/${LOCK_NAME}"
# Installers here run with a TMPDIR other than the uninstaller's.
INSTALLER_TMPDIR="${TEMP_ROOT}/installer-tmp"
/bin/mkdir -p "${INSTALLER_TMPDIR}"
HELD_REASON='an installer holds the install lock'
UNVERIFIED_REASON='the install lock cannot be verified'
HELD_NOTE="${HELD_REASON}: a confirmed uninstall stops without changing anything until the installation ends."
HELD_ERROR="Error: ${HELD_REASON}; nothing was changed. Rerun the uninstaller after the installation ends."
LOCKED_OUTPUT="${TEMP_ROOT}/locked-output.txt"

LOCKED_UNINSTALLER="${UNINSTALL_COPY}"
run_locked_uninstall() {
    HOME="${LOCKED_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
        /bin/bash "${LOCKED_UNINSTALLER}" "$@"
}

# What the uninstaller's probe can use, for failure messages.
describe_install_lock() {
    /usr/bin/printf '/usr/bin/lockf %s; /usr/bin/perl %s; macOS %s; locks: %s' \
        "$([ -x /usr/bin/lockf ] && echo present || echo missing)" \
        "$([ -x /usr/bin/perl ] && echo present || echo missing)" \
        "$(/usr/bin/sw_vers -productVersion 2>/dev/null || echo unknown)" \
        "$(/bin/ls -ld "${INSTALL_LOCK}" "${TMPDIR_LOCK}" "${SHARED_TMP_LOCK}" 2>&1 | /usr/bin/tr '\n' ';' || true)"
}

# The uninstaller probes the lock with lockf(1) where it exists (macOS 15 and
# later) and with perl's flock otherwise. This copy has no lockf, so the perl
# path runs on every host; the plain copy uses whichever the host has.
# NO_PROBE_UNINSTALLER has neither. SCRIPTED_PROBE_UNINSTALLER's lockf takes
# no lock and answers as SCRIPTED_LOCKF_MODE says.
PERL_PROBE_UNINSTALLER="${TEMP_ROOT}/uninstall-perl-probe.sh"
NO_PROBE_UNINSTALLER="${TEMP_ROOT}/uninstall-no-probe.sh"
SCRIPTED_PROBE_UNINSTALLER="${TEMP_ROOT}/uninstall-scripted-probe.sh"
/usr/bin/sed -e "s|/usr/bin/lockf|${TEMP_ROOT}/missing-lockf|g" "${UNINSTALL_COPY}" > "${PERL_PROBE_UNINSTALLER}"
/usr/bin/sed -e "s|/usr/bin/perl|${TEMP_ROOT}/missing-perl|g" "${PERL_PROBE_UNINSTALLER}" > "${NO_PROBE_UNINSTALLER}"
/usr/bin/sed -e "s|/usr/bin/lockf|${FAKE_BIN}/scripted-lockf|g" "${UNINSTALL_COPY}" > "${SCRIPTED_PROBE_UNINSTALLER}"
/usr/bin/grep -F '/usr/bin/lockf' "${UNINSTALL_COPY}" >/dev/null &&
    /usr/bin/grep -F '/usr/bin/perl' "${UNINSTALL_COPY}" >/dev/null ||
    fail 'uninstaller no longer names the lock probes this test replaces'
# Only the descriptor form, as the uninstaller calls it. The first call is
# free: free-once takes it as it is, replace-once first puts a new file at the
# install lock's path, as an uninstaller's removal and a new installer would.
# Every later call reports a held lock, except that replace-always replaces
# the file and reports it free on every call. remove-once removes the file
# instead, as another uninstaller that held it would.
SCRIPTED_LOCKF_MODE="${TEMP_ROOT}/scripted-lockf-mode"
SCRIPTED_LOCKF_CALLS="${TEMP_ROOT}/scripted-lockf-calls"
/bin/cat > "${FAKE_BIN}/scripted-lockf" <<EOF
#!/bin/bash
[ "\$#" -eq 4 ] && [ "\$1" = -s ] && [ "\$2" = -t ] && [ "\$3" = 0 ] || exit 64
count=\$(( \$(/bin/cat '${SCRIPTED_LOCKF_CALLS}' 2>/dev/null || echo 0) + 1 ))
/usr/bin/printf '%s\n' "\${count}" > '${SCRIPTED_LOCKF_CALLS}'
mode="\$(/bin/cat '${SCRIPTED_LOCKF_MODE}')"
[ "\${count}" -eq 1 ] || [ "\${mode}" = replace-always ] || exit 75
case "\${mode}" in
    free-once) exit 0 ;;
    remove-once)
        /bin/rm -f '${INSTALL_LOCK}'
        exit 0
        ;;
    replace-once|replace-always)
        /usr/bin/printf 'replacement\n' > '${INSTALL_LOCK}.next'
        /bin/mv -f '${INSTALL_LOCK}.next' '${INSTALL_LOCK}'
        exit 0
        ;;
esac
exit 71
EOF
/bin/chmod 755 "${PERL_PROBE_UNINSTALLER}" "${NO_PROBE_UNINSTALLER}" "${SCRIPTED_PROBE_UNINSTALLER}" \
    "${FAKE_BIN}/scripted-lockf"
use_scripted_lockf() {
    /usr/bin/printf '%s\n' "$1" > "${SCRIPTED_LOCKF_MODE}"
    /bin/rm -f "${SCRIPTED_LOCKF_CALLS}"
    LOCKED_UNINSTALLER="${SCRIPTED_PROBE_UNINSTALLER}"
}

assert_locked_temps_preserved_in_plan() {
    local reason="$1"
    local output="${TEMP_ROOT}/locked-dry-run.txt"
    run_locked_uninstall --dry-run > "${output}"
    for path in "${LOCKED_TEMPS[@]}"; do
        /usr/bin/grep -F -x "  preserve ${path} (${reason})" "${output}" >/dev/null ||
            fail "dry-run did not preserve ${path} (${reason}); plan: $(/usr/bin/grep -F "${path}" "${output}" || true); $(describe_install_lock)"
        /usr/bin/grep -F -x "  remove ${path}" "${output}" >/dev/null &&
            fail "dry-run listed ${path} for removal (${reason})"
        [ -e "${path}" ] || fail "dry-run removed ${path}"
    done
    /usr/bin/grep -F -x "  remove ${LOCKED_UNINSTALL_TEMP}" "${output}" >/dev/null ||
        fail "dry-run gated the uninstaller's own copy on the install lock (${reason})"
    if [ "${reason}" = "${HELD_REASON}" ]; then
        /usr/bin/grep -F -x "${HELD_NOTE}" "${output}" >/dev/null ||
            fail "dry-run did not say that a confirmed uninstall would stop"
    else
        /usr/bin/grep -F "${HELD_REASON}:" "${output}" >/dev/null &&
            fail "dry-run said an installer holds the lock (${reason})"
    fi
    return 0
}

# Everything a confirmed run could change, to prove it changed nothing.
locked_state() {
    /usr/bin/find "${LOCKED_HOME}" "${SYSTEM_APPS}" "${FAKE_DARWIN_TMP}" "${TEMP_ROOT}/tmp" \
        "${FAKE_SHARED_TMP}" -exec /usr/bin/stat -f '%N %i %m %z %Lp' {} + | /usr/bin/sort
}

# While an installer holds a lock, the dry run says the uninstall would stop,
# and the confirmed run fails before changing anything.
assert_uninstall_refused() {
    local holder="$1"
    local before
    assert_locked_temps_preserved_in_plan "${HELD_REASON}"
    before="$(locked_state)"
    if run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1; then
        fail "uninstall reported success while ${holder} held the install lock"
    fi
    /usr/bin/grep -F -x "${HELD_ERROR}" "${LOCKED_OUTPUT}" >/dev/null ||
        fail "uninstall did not stop for ${holder}: $(/bin/cat "${LOCKED_OUTPUT}"); $(describe_install_lock)"
    [ "$(locked_state)" = "${before}" ] ||
        fail "uninstall changed files while ${holder} held the install lock: $(/usr/bin/diff <(/usr/bin/printf '%s\n' "${before}") <(locked_state) || true)"
}

# Lock holders run the installer's own lock (install.sh's marked lines, with
# the fake getconf) in /bin/bash, or act as an installer from before the
# per-user temporary directory lock, which flocked the file at a given path.
# A holder keeps its lock until this test closes the write end of its pipe
# (descriptor 7), which also happens if the test dies.
INSTALL_LOCK_BLOCK="${TEMP_ROOT}/install-lock-block.sh"
/usr/bin/awk '
    $0 == "# >>> install lock" { inside = 1; starts++; next }
    $0 == "# <<< install lock" { inside = 0; ends++; next }
    inside { print }
    END { exit (starts == 1 && ends == 1) ? 0 : 1 }
' "${INSTALL_SCRIPT}" | /usr/bin/sed -e "s|/usr/bin/getconf|${FAKE_BIN}/getconf|g" > "${INSTALL_LOCK_BLOCK}" ||
    fail 'could not extract the install lock block of install.sh'
/usr/bin/grep -F 'acquire_install_lock() {' "${INSTALL_LOCK_BLOCK}" >/dev/null ||
    fail 'the install lock block defines no acquire_install_lock'
/usr/bin/grep -E '(^|[^[:alnum:]_./-])getconf([^[:alnum:]_.-]|$)|/usr/bin/getconf' "${INSTALL_LOCK_BLOCK}" |
    /usr/bin/grep -v -E '^[[:space:]]*#' | /usr/bin/grep -v -F "${FAKE_BIN}/getconf" >/dev/null &&
    fail 'the install lock block copy can still reach the real getconf'
LOCK_HOLDER_SCRIPT="${TEMP_ROOT}/installer-lock-holder.sh"
/bin/cat > "${LOCK_HOLDER_SCRIPT}" <<'EOF'
#!/bin/bash
set -euo pipefail
if [ "$1" = installer ]; then
    source "$2"
    acquire_install_lock || exit 1
else
    exec 9>>"$2"
    /usr/bin/perl -MFcntl=:flock -e 'open(my $lock, "<&=", 9) or exit 71; flock($lock, LOCK_EX | LOCK_NB) or exit 75;'
fi
/usr/bin/touch "$3"
read -r _ || true
EOF
start_lock_holder() {
    local ready="${TEMP_ROOT}/lock-ready"
    local fifo="${TEMP_ROOT}/lock-release.fifo"
    /bin/rm -f "${ready}" "${fifo}"
    /usr/bin/mkfifo "${fifo}"
    TMPDIR="${INSTALLER_TMPDIR}" /bin/bash "${LOCK_HOLDER_SCRIPT}" "$1" "$2" "${ready}" \
        < "${fifo}" > "${TEMP_ROOT}/lock-holder.log" 2>&1 7>&- &
    LOCK_HOLDER=$!
    exec 7>"${fifo}"
    local attempt=0
    until [ -e "${ready}" ]; do
        attempt=$((attempt + 1))
        [ "${attempt}" -lt 400 ] ||
            fail "lock holder ($1) did not take the lock: $(/bin/cat "${TEMP_ROOT}/lock-holder.log"); $(describe_install_lock)"
        /bin/sleep 0.025
    done
}
release_lock_holder() {
    exec 7>&-
    wait "${LOCK_HOLDER}" || fail "lock holder failed: $(/bin/cat "${TEMP_ROOT}/lock-holder.log")"
    LOCK_HOLDER=""
}

# The installer of this change, started with another TMPDIR: once through the
# perl probe and once through the host's own.
start_lock_holder installer "${INSTALL_LOCK_BLOCK}"
[ -f "${INSTALL_LOCK}" ] || fail "the installer did not lock ${INSTALL_LOCK}; $(describe_install_lock)"
[ ! -e "${INSTALLER_TMPDIR}/${LOCK_NAME}" ] || fail 'the installer locked a file in its TMPDIR'
for LOCKED_UNINSTALLER in "${PERL_PROBE_UNINSTALLER}" "${UNINSTALL_COPY}"; do
    assert_uninstall_refused 'an installer'
done
LOCKED_UNINSTALLER="${UNINSTALL_COPY}"
release_lock_holder

# Installers from before this change locked ${TMPDIR:-/tmp}'s file: here the
# uninstaller's TMPDIR, and /tmp for an installer run without TMPDIR.
for legacy_lock in "${TMPDIR_LOCK}" "${SHARED_TMP_LOCK}"; do
    start_lock_holder legacy "${legacy_lock}"
    assert_uninstall_refused "a legacy installer (${legacy_lock})"
    release_lock_holder
    /bin/rm -f "${legacy_lock}"
done

# An installer that takes the lock after the uninstaller's first check still
# keeps its leftovers, and its lock file: the uninstall proceeds, reports each,
# and fails.
[ -f "${INSTALL_LOCK}" ] || fail 'the free install lock file is missing'
use_scripted_lockf free-once
install_fake_helper "${LOCKED_HOME}"
if run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1; then
    fail 'uninstall reported success while an installer that started late held the install lock'
fi
/usr/bin/grep -F -x "${HELD_ERROR}" "${LOCKED_OUTPUT}" >/dev/null &&
    fail 'uninstall stopped although the install lock was free when it started'
for path in "${LOCKED_TEMPS[@]}"; do
    assert_exists "${path}"
    /usr/bin/grep -F -x "Warning: preserving installer staging because ${HELD_REASON}: ${path}" \
        "${LOCKED_OUTPUT}" >/dev/null || fail "uninstall did not report preserving ${path}: $(/bin/cat "${LOCKED_OUTPUT}")"
done
assert_exists "${SYSTEM_APPS}/.codex-monitor-backup.L0cK02/Codex Monitor.app/Contents/Info.plist"
assert_exists "${INSTALL_LOCK}"
/usr/bin/grep -F -x "Warning: lock is still held; preserving: ${INSTALL_LOCK}" "${LOCKED_OUTPUT}" >/dev/null ||
    fail 'uninstall did not keep the held install lock'
assert_absent "${LOCKED_UNINSTALL_TEMP}"
make_file 600 "${LOCKED_UNINSTALL_TEMP}"

# A probe must lock the file that the path names. Here the path gets a new
# file, which an installer holds, while the probe opens the old one.
use_scripted_lockf replace-once
assert_locked_temps_preserved_in_plan "${HELD_REASON}"
[ "$(/bin/cat "${INSTALL_LOCK}")" = replacement ] || fail 'the scripted lockf did not replace the lock file'
# A lock file that keeps changing under the probe cannot be verified.
use_scripted_lockf replace-always
assert_locked_temps_preserved_in_plan "${UNVERIFIED_REASON}"
# One that another uninstaller removed while the probe opened it was free.
use_scripted_lockf remove-once
run_locked_uninstall --dry-run > "${TEMP_ROOT}/removed-lock-dry-run.txt"
for path in "${LOCKED_TEMPS[@]}"; do
    /usr/bin/grep -F -x "  remove ${path}" "${TEMP_ROOT}/removed-lock-dry-run.txt" >/dev/null ||
        fail "dry-run kept ${path} behind a lock file that was removed while free"
done
assert_absent "${INSTALL_LOCK}"
LOCKED_UNINSTALLER="${UNINSTALL_COPY}"
/bin/rm -f "${INSTALL_LOCK}"

# A lock path that is a symlink, a directory, or unreadable cannot prove that
# no installer is running.
/bin/ln -s "${OUTSIDE}/file-target" "${INSTALL_LOCK}"
assert_locked_temps_preserved_in_plan "${UNVERIFIED_REASON}"
# Nothing else fails in this run: the lock-file cleanup removes the symlink
# itself, so only the preserved leftovers can make the uninstall report
# warnings.
install_fake_helper "${LOCKED_HOME}"
if run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1; then
    fail 'uninstall reported success although it kept installer leftovers'
fi
for path in "${LOCKED_TEMPS[@]}"; do
    assert_exists "${path}"
    /usr/bin/grep -F -x "Warning: preserving installer staging because ${UNVERIFIED_REASON}: ${path}" \
        "${LOCKED_OUTPUT}" >/dev/null || fail "uninstall did not report preserving ${path} behind an unverifiable lock"
done
/usr/bin/grep -F 'Warning:' "${LOCKED_OUTPUT}" | /usr/bin/grep -v -F -e 'preserving installer staging' \
    -e 'uninstall completed with warnings' >/dev/null &&
    fail "an unrelated warning masks the preserved-leftover failure: $(/bin/cat "${LOCKED_OUTPUT}")"
assert_absent "${INSTALL_LOCK}"
assert_content 'outside file sentinel' "${OUTSIDE}/file-target"
assert_absent "${LOCKED_UNINSTALL_TEMP}"
make_file 600 "${LOCKED_UNINSTALL_TEMP}"
/bin/mkdir "${INSTALL_LOCK}"
assert_locked_temps_preserved_in_plan "${UNVERIFIED_REASON}"
/bin/rmdir "${INSTALL_LOCK}"
/usr/bin/touch "${INSTALL_LOCK}"
/bin/chmod 000 "${INSTALL_LOCK}"
[ ! -r "${INSTALL_LOCK}" ] || fail 'this test cannot make an unreadable file; do not run it as root'
assert_locked_temps_preserved_in_plan "${UNVERIFIED_REASON}"
install_fake_helper "${LOCKED_HOME}"
if run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1; then
    fail 'uninstall reported success behind an unreadable install lock'
fi
for path in "${LOCKED_TEMPS[@]}"; do
    assert_exists "${path}"
done
/usr/bin/grep -F -x "Warning: cannot tell whether the lock is held; preserving: ${INSTALL_LOCK}" \
    "${LOCKED_OUTPUT}" >/dev/null || fail 'uninstall removed or did not report an unreadable lock file'
make_file 600 "${LOCKED_UNINSTALL_TEMP}"
/bin/chmod 600 "${INSTALL_LOCK}"
# Nor can a free lock file be probed with neither lockf nor perl: the
# confirmed run keeps the leftovers and the lock file itself.
LOCKED_UNINSTALLER="${NO_PROBE_UNINSTALLER}"
assert_locked_temps_preserved_in_plan "${UNVERIFIED_REASON}"
install_fake_helper "${LOCKED_HOME}"
if run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1; then
    fail 'uninstall reported success without a way to probe the install lock'
fi
for path in "${LOCKED_TEMPS[@]}"; do
    assert_exists "${path}"
done
assert_exists "${INSTALL_LOCK}"
/usr/bin/grep -F -x "Warning: cannot tell whether the lock is held; preserving: ${INSTALL_LOCK}" \
    "${LOCKED_OUTPUT}" >/dev/null || fail 'uninstall removed or did not report an unprobed lock file'
make_file 600 "${LOCKED_UNINSTALL_TEMP}"
LOCKED_UNINSTALLER="${UNINSTALL_COPY}"

# Free lock files prove that their installers have exited. A dry run leaves
# them as they are; a confirmed run removes them with the leftovers. This run
# takes the perl path; the next one the host's own probe.
/usr/bin/touch "${TMPDIR_LOCK}" "${SHARED_TMP_LOCK}"
FREE_LOCKS=("${INSTALL_LOCK}" "${TMPDIR_LOCK}" "${SHARED_TMP_LOCK}")
free_lock_state() { /usr/bin/stat -f '%N %i %m %z' "${FREE_LOCKS[@]}"; }
FREE_LOCK_STATE="$(free_lock_state)"
for LOCKED_UNINSTALLER in "${PERL_PROBE_UNINSTALLER}" "${UNINSTALL_COPY}"; do
    run_locked_uninstall --dry-run > "${TEMP_ROOT}/free-dry-run.txt"
    for path in "${LOCKED_TEMPS[@]}" "${FREE_LOCKS[@]}"; do
        /usr/bin/grep -F -x "  remove ${path}" "${TEMP_ROOT}/free-dry-run.txt" >/dev/null ||
            fail "dry-run did not plan to remove ${path} behind free locks"
    done
    [ "$(free_lock_state)" = "${FREE_LOCK_STATE}" ] || fail 'a dry run changed a free lock file'
done
LOCKED_UNINSTALLER="${PERL_PROBE_UNINSTALLER}"
install_fake_helper "${LOCKED_HOME}"
run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1 ||
    fail "uninstall failed behind free locks: $(/bin/cat "${LOCKED_OUTPUT}")"
for path in "${LOCKED_TEMPS[@]}" "${FREE_LOCKS[@]}" "${LOCKED_UNINSTALL_TEMP}"; do
    assert_absent "${path}"
done
/usr/bin/touch "${INSTALL_LOCK}"
LOCKED_UNINSTALLER="${UNINSTALL_COPY}"
install_fake_helper "${LOCKED_HOME}"
run_locked_uninstall --yes > "${LOCKED_OUTPUT}" 2>&1 ||
    fail "uninstall failed behind a free lock through the host's probe: $(/bin/cat "${LOCKED_OUTPUT}")"
assert_absent "${INSTALL_LOCK}"

# ------------------------------------------------------------------------------
# Without an absolute per-user temporary directory the uninstaller cannot look
# for remote-install clones, nor probe the install lock kept there. It must
# say so rather than guess a location, and keep every installer leftover.
# ------------------------------------------------------------------------------
GETCONF_HOME="${TEMP_ROOT}/getconf-home"
GETCONF_CWD="${TEMP_ROOT}/getconf-cwd"
GETCONF_CLONE="${FAKE_DARWIN_TMP}/codex-mon-install-XXXXXX.GcF01abcde"
RELATIVE_CLONE="${GETCONF_CWD}/relative/codex-mon-install-XXXXXX.GcF02abcde"
GETCONF_STAGING="${GETCONF_HOME}/.local/bin/.codex-mon.install.GcF03a"
/bin/mkdir -p "${GETCONF_HOME}/.codex" "${GETCONF_HOME}/.local/bin" "${GETCONF_CWD}/relative"
make_staging_root 700 "${GETCONF_CLONE}" 'Cargo.toml'
make_staging_root 700 "${RELATIVE_CLONE}" 'Cargo.toml'
make_file 600 "${GETCONF_STAGING}"
for getconf_output in missing 'relative/'; do
    if [ "${getconf_output}" = missing ]; then
        /bin/rm -f "${GETCONF_OUTPUT}"
    else
        /usr/bin/printf '%s\n' "${getconf_output}" > "${GETCONF_OUTPUT}"
    fi
    install_fake_helper "${GETCONF_HOME}"
    GETCONF_DRY_RUN="${TEMP_ROOT}/getconf-dry-run.txt"
    (cd "${GETCONF_CWD}" && HOME="${GETCONF_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
        /bin/bash "${UNINSTALL_COPY}" --dry-run > "${GETCONF_DRY_RUN}")
    /usr/bin/grep -F -x '  preserve remote-install clones (the per-user temporary directory is unknown)' \
        "${GETCONF_DRY_RUN}" >/dev/null || fail "dry-run hid an unknown temporary directory (${getconf_output})"
    /usr/bin/grep -F 'codex-mon-install-XXXXXX' "${GETCONF_DRY_RUN}" >/dev/null &&
        fail "dry-run listed a clone without a verified temporary directory (${getconf_output})"
    /usr/bin/grep -F -x "  preserve ${GETCONF_STAGING} (the install lock cannot be verified)" \
        "${GETCONF_DRY_RUN}" >/dev/null || fail "dry-run did not keep staging behind an unknown lock (${getconf_output})"
    if (cd "${GETCONF_CWD}" && HOME="${GETCONF_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
        /bin/bash "${UNINSTALL_COPY}" --yes > "${TEMP_ROOT}/getconf-output.txt" 2>&1); then
        fail "uninstall reported success without checking for remote-install clones (${getconf_output})"
    fi
    /usr/bin/grep -F 'Warning: the per-user temporary directory is unknown; remote-install clones were not checked' \
        "${TEMP_ROOT}/getconf-output.txt" >/dev/null || fail "uninstall hid an unknown temporary directory (${getconf_output})"
    /usr/bin/grep -F -x "Warning: preserving installer staging because the install lock cannot be verified: ${GETCONF_STAGING}" \
        "${TEMP_ROOT}/getconf-output.txt" >/dev/null || fail "uninstall did not keep staging behind an unknown lock (${getconf_output})"
    assert_exists "${GETCONF_CLONE}/Cargo.toml"
    assert_exists "${RELATIVE_CLONE}/Cargo.toml"
    assert_exists "${GETCONF_STAGING}"
done
/usr/bin/printf '%s/\n' "${FAKE_DARWIN_TMP}" > "${GETCONF_OUTPUT}"
/bin/rm -rf -- "${GETCONF_CLONE}"

PURGE_HOME="${TEMP_ROOT}/purge-home"
/bin/mkdir -p "${PURGE_HOME}/.codex"
install_fake_helper "${PURGE_HOME}"
/usr/bin/printf 'purge me\n' > "${PURGE_HOME}/.codex/accounts.json"
/bin/chmod 600 "${PURGE_HOME}/.codex/accounts.json"
HOME="${PURGE_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes --purge-data >/dev/null
assert_absent "${PURGE_HOME}/.codex/accounts.json"

PURGE_SYMLINK_HOME="${TEMP_ROOT}/purge-symlink-home"
PURGE_EXTERNAL="${TEMP_ROOT}/purge-external"
/bin/mkdir -p "${PURGE_SYMLINK_HOME}/.codex" "${PURGE_EXTERNAL}"
install_fake_helper "${PURGE_SYMLINK_HOME}"
/usr/bin/printf 'external account sentinel\n' > "${PURGE_EXTERNAL}/accounts.json"
/bin/ln -s "${PURGE_EXTERNAL}/accounts.json" "${PURGE_SYMLINK_HOME}/.codex/accounts.json"
if HOME="${PURGE_SYMLINK_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes --purge-data >/dev/null 2>&1; then
    fail 'purge accepted a symlinked accounts.json'
fi
assert_content 'external account sentinel' "${PURGE_EXTERNAL}/accounts.json"
[ -L "${PURGE_SYMLINK_HOME}/.codex/accounts.json" ] || fail 'purge removed symlinked accounts.json'

PURGE_DIRECTORY_HOME="${TEMP_ROOT}/purge-directory-home"
/bin/mkdir -p "${PURGE_DIRECTORY_HOME}/.codex/accounts.json"
install_fake_helper "${PURGE_DIRECTORY_HOME}"
if HOME="${PURGE_DIRECTORY_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes --purge-data >/dev/null 2>&1; then
    fail 'purge accepted a directory accounts.json'
fi
assert_exists "${PURGE_DIRECTORY_HOME}/.codex/accounts.json"

# A symlinked log parent must fail closed without printing or deleting the
# external switcher log it targets.
SYMLINK_LOG_HOME="${TEMP_ROOT}/symlink-log-home"
EXTERNAL_LOG_ROOT="${TEMP_ROOT}/external-log"
/bin/mkdir -p "${SYMLINK_LOG_HOME}/.codex" "${EXTERNAL_LOG_ROOT}/archive"
install_fake_helper "${SYMLINK_LOG_HOME}"
/usr/bin/printf 'external switcher sentinel\n' > "${EXTERNAL_LOG_ROOT}/switcher.log"
/bin/ln -s "${EXTERNAL_LOG_ROOT}" "${SYMLINK_LOG_HOME}/.codex/log"
SYMLINK_LOG_OUTPUT="${TEMP_ROOT}/symlink-log-output.txt"
HOME="${SYMLINK_LOG_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --dry-run > "${SYMLINK_LOG_OUTPUT}"
/usr/bin/grep -F 'preserve Monitor log/recovery cleanup' "${SYMLINK_LOG_OUTPUT}" >/dev/null
/usr/bin/grep -F "${EXTERNAL_LOG_ROOT}" "${SYMLINK_LOG_OUTPUT}" >/dev/null &&
    fail 'dry-run exposed the external log target'
if HOME="${SYMLINK_LOG_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes >/dev/null 2>&1; then
    fail 'uninstall accepted a symlinked log parent'
fi
assert_content 'external switcher sentinel' "${EXTERNAL_LOG_ROOT}/switcher.log"

# A symlinked recovery parent must also preserve its external cancellation
# marker while reporting a failed cleanup.
SYMLINK_RECOVERY_HOME="${TEMP_ROOT}/symlink-recovery-home"
EXTERNAL_RECOVERY_ROOT="${TEMP_ROOT}/external-recovery"
/bin/mkdir -p "${SYMLINK_RECOVERY_HOME}/.codex" "${EXTERNAL_RECOVERY_ROOT}"
install_fake_helper "${SYMLINK_RECOVERY_HOME}"
/usr/bin/printf 'external cancel sentinel\n' > "${EXTERNAL_RECOVERY_ROOT}/cancel-restart"
/bin/ln -s "${EXTERNAL_RECOVERY_ROOT}" "${SYMLINK_RECOVERY_HOME}/.codex/recovery-runs"
SYMLINK_RECOVERY_OUTPUT="${TEMP_ROOT}/symlink-recovery-output.txt"
HOME="${SYMLINK_RECOVERY_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --dry-run > "${SYMLINK_RECOVERY_OUTPUT}"
/usr/bin/grep -F 'preserve Monitor log/recovery cleanup' "${SYMLINK_RECOVERY_OUTPUT}" >/dev/null
/usr/bin/grep -F "${EXTERNAL_RECOVERY_ROOT}" "${SYMLINK_RECOVERY_OUTPUT}" >/dev/null &&
    fail 'dry-run exposed the external recovery target'
if HOME="${SYMLINK_RECOVERY_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    /bin/bash "${UNINSTALL_COPY}" --yes >/dev/null 2>&1; then
    fail 'uninstall accepted a symlinked recovery parent'
fi
assert_content 'external cancel sentinel' "${EXTERNAL_RECOVERY_ROOT}/cancel-restart"

# CODEX_HOME itself is an anchor. A symlinked anchor must be rejected before
# canonicalization, otherwise every child path could resolve into external
# state that the uninstaller does not own.
SYMLINK_ANCHOR_HOME="${TEMP_ROOT}/symlink-anchor-home"
EXTERNAL_ANCHOR_ROOT="${TEMP_ROOT}/external-anchor"
/bin/mkdir -p "${SYMLINK_ANCHOR_HOME}" "${EXTERNAL_ANCHOR_ROOT}"
install_fake_helper "${SYMLINK_ANCHOR_HOME}"
/usr/bin/printf 'external anchor sentinel\n' > "${EXTERNAL_ANCHOR_ROOT}/accounts.json"
/bin/ln -s "${EXTERNAL_ANCHOR_ROOT}" "${SYMLINK_ANCHOR_HOME}/.codex"
SYMLINK_ANCHOR_OUTPUT="${TEMP_ROOT}/symlink-anchor-output.txt"
if HOME="${SYMLINK_ANCHOR_HOME}" CODEX_HOME="" TMPDIR="${TEMP_ROOT}/tmp" \
    PATH="${FAKE_BIN}:${PATH}" /bin/bash "${UNINSTALL_COPY}" --dry-run > "${SYMLINK_ANCHOR_OUTPUT}"; then
    fail 'uninstall accepted a symlinked CODEX_HOME anchor'
fi
/usr/bin/grep -F 'refusing to operate through a symlinked CODEX_HOME' "${SYMLINK_ANCHOR_OUTPUT}" >/dev/null
/usr/bin/grep -F "${EXTERNAL_ANCHOR_ROOT}" "${SYMLINK_ANCHOR_OUTPUT}" >/dev/null &&
    fail 'symlinked CODEX_HOME exposed its external target'
assert_content 'external anchor sentinel' "${EXTERNAL_ANCHOR_ROOT}/accounts.json"

printf 'log permissions and uninstall cleanup: ok\n'
