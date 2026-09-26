#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RAW_TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/codex-monitor-log-test.XXXXXX")"
TEMP_ROOT="$(cd "${RAW_TEMP_ROOT}" && /bin/pwd -P)"
cleanup() {
    /bin/rm -rf -- "${TEMP_ROOT}"
}
trap cleanup EXIT INT TERM

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
for name in "${STAGING_LOOKALIKES_0600[@]}"; do /bin/chmod 600 "${FAKE_HOME}/.codex/${name}"; done
for name in "${STAGING_LOOKALIKES_0644[@]}"; do /bin/chmod 644 "${FAKE_HOME}/.codex/${name}"; done
[ "$(/usr/bin/stat -f '%Lp' "${STAGING_SYMLINK}")" = 600 ] || fail 'symlink look-alike mode setup failed'
/bin/chmod 755 "${FAKE_BIN}/launchctl" "${FAKE_BIN}/osascript" "${FAKE_BIN}/defaults" "${FAKE_BIN}/lsregister"

UNINSTALL_COPY="${TEMP_ROOT}/uninstall.sh"
/usr/bin/sed \
    -e "s|/Applications/|${FAKE_ROOT}/Applications/|g" \
    -e "s|/bin/launchctl|${FAKE_BIN}/launchctl|g" \
    -e "s|/usr/bin/osascript|${FAKE_BIN}/osascript|g" \
    -e "s|/usr/bin/defaults|${FAKE_BIN}/defaults|g" \
    -e "s|/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister|${FAKE_BIN}/lsregister|g" \
    "${PROJECT_DIR}/scripts/uninstall.sh" > "${UNINSTALL_COPY}"
/bin/chmod 755 "${UNINSTALL_COPY}"

DRY_RUN_OUTPUT="${TEMP_ROOT}/dry-run.txt"
HOME="${FAKE_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --dry-run > "${DRY_RUN_OUTPUT}"
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

HOME="${FAKE_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --yes >/dev/null
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

PURGE_HOME="${TEMP_ROOT}/purge-home"
/bin/mkdir -p "${PURGE_HOME}/.codex"
install_fake_helper "${PURGE_HOME}"
/usr/bin/printf 'purge me\n' > "${PURGE_HOME}/.codex/accounts.json"
/bin/chmod 600 "${PURGE_HOME}/.codex/accounts.json"
HOME="${PURGE_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --yes --purge-data >/dev/null
assert_absent "${PURGE_HOME}/.codex/accounts.json"

PURGE_SYMLINK_HOME="${TEMP_ROOT}/purge-symlink-home"
PURGE_EXTERNAL="${TEMP_ROOT}/purge-external"
/bin/mkdir -p "${PURGE_SYMLINK_HOME}/.codex" "${PURGE_EXTERNAL}"
install_fake_helper "${PURGE_SYMLINK_HOME}"
/usr/bin/printf 'external account sentinel\n' > "${PURGE_EXTERNAL}/accounts.json"
/bin/ln -s "${PURGE_EXTERNAL}/accounts.json" "${PURGE_SYMLINK_HOME}/.codex/accounts.json"
if HOME="${PURGE_SYMLINK_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --yes --purge-data >/dev/null 2>&1; then
    fail 'purge accepted a symlinked accounts.json'
fi
assert_content 'external account sentinel' "${PURGE_EXTERNAL}/accounts.json"
[ -L "${PURGE_SYMLINK_HOME}/.codex/accounts.json" ] || fail 'purge removed symlinked accounts.json'

PURGE_DIRECTORY_HOME="${TEMP_ROOT}/purge-directory-home"
/bin/mkdir -p "${PURGE_DIRECTORY_HOME}/.codex/accounts.json"
install_fake_helper "${PURGE_DIRECTORY_HOME}"
if HOME="${PURGE_DIRECTORY_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --yes --purge-data >/dev/null 2>&1; then
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
    "${UNINSTALL_COPY}" --dry-run > "${SYMLINK_LOG_OUTPUT}"
/usr/bin/grep -F 'preserve Monitor log/recovery cleanup' "${SYMLINK_LOG_OUTPUT}" >/dev/null
/usr/bin/grep -F "${EXTERNAL_LOG_ROOT}" "${SYMLINK_LOG_OUTPUT}" >/dev/null &&
    fail 'dry-run exposed the external log target'
if HOME="${SYMLINK_LOG_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --yes >/dev/null 2>&1; then
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
    "${UNINSTALL_COPY}" --dry-run > "${SYMLINK_RECOVERY_OUTPUT}"
/usr/bin/grep -F 'preserve Monitor log/recovery cleanup' "${SYMLINK_RECOVERY_OUTPUT}" >/dev/null
/usr/bin/grep -F "${EXTERNAL_RECOVERY_ROOT}" "${SYMLINK_RECOVERY_OUTPUT}" >/dev/null &&
    fail 'dry-run exposed the external recovery target'
if HOME="${SYMLINK_RECOVERY_HOME}" TMPDIR="${TEMP_ROOT}/tmp" PATH="${FAKE_BIN}:${PATH}" \
    "${UNINSTALL_COPY}" --yes >/dev/null 2>&1; then
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
    PATH="${FAKE_BIN}:${PATH}" "${UNINSTALL_COPY}" --dry-run > "${SYMLINK_ANCHOR_OUTPUT}"; then
    fail 'uninstall accepted a symlinked CODEX_HOME anchor'
fi
/usr/bin/grep -F 'refusing to operate through a symlinked CODEX_HOME' "${SYMLINK_ANCHOR_OUTPUT}" >/dev/null
/usr/bin/grep -F "${EXTERNAL_ANCHOR_ROOT}" "${SYMLINK_ANCHOR_OUTPUT}" >/dev/null &&
    fail 'symlinked CODEX_HOME exposed its external target'
assert_content 'external anchor sentinel' "${EXTERNAL_ANCHOR_ROOT}/accounts.json"

printf 'log permissions and uninstall cleanup: ok\n'
