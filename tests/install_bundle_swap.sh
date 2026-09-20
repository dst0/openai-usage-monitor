#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-monitor-swap-test.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT INT TERM

source "${PROJECT_DIR}/scripts/install_bundle_swap.sh"

reset_state() {
    APP_STAGE_ROOT=""
    APP_STAGING_PATH=""
    APP_BACKUP_ROOT=""
    APP_BACKUP_PATH=""
    APP_TARGET_PATH=""
    APP_SWAP_ACTIVE=0
    APP_HAD_EXISTING_TARGET=0
}

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

INSTALL_DIR="${TEMP_ROOT}/Applications"
BUNDLE_NAME="Codex Monitor.app"
TARGET="${INSTALL_DIR}/${BUNDLE_NAME}"
/bin/mkdir -p "${INSTALL_DIR}" "${TARGET}"
/usr/bin/printf 'old\n' > "${TARGET}/version"

reset_state
if prepare_app_bundle_staging "${TEMP_ROOT}/missing.app" "${INSTALL_DIR}" "${BUNDLE_NAME}" 2>/dev/null; then
    fail 'staging unexpectedly accepted a missing source bundle'
fi
[ "$(/bin/cat "${TARGET}/version")" = old ] || fail 'copy failure changed the installed app'

SOURCE="${TEMP_ROOT}/new.app"
/bin/mkdir -p "${SOURCE}"
/usr/bin/printf 'new\n' > "${SOURCE}/version"
reset_state
prepare_app_bundle_staging "${SOURCE}" "${INSTALL_DIR}" "${BUNDLE_NAME}"
APP_BACKUP_PATH="${TEMP_ROOT}/missing-parent/${BUNDLE_NAME}"
if activate_app_bundle_staging "${TARGET}" 2>/dev/null; then
    fail 'activation unexpectedly accepted an unavailable backup path'
fi
[ "$(/bin/cat "${TARGET}/version")" = old ] || fail 'backup-move failure removed the old app'
cleanup_app_bundle_swap_paths

reset_state
INTERRUPT_INSTALL_DIR="${TEMP_ROOT}/InterruptApplications"
INTERRUPT_TARGET="${INTERRUPT_INSTALL_DIR}/${BUNDLE_NAME}"
/bin/mkdir -p "${INTERRUPT_TARGET}"
/usr/bin/printf 'old\n' > "${INTERRUPT_TARGET}/version"
prepare_app_bundle_staging "${SOURCE}" "${INTERRUPT_INSTALL_DIR}" "${BUNDLE_NAME}"
APP_TARGET_PATH="${INTERRUPT_TARGET}"
APP_HAD_EXISTING_TARGET=1
APP_SWAP_ACTIVE=1
/bin/mv "${INTERRUPT_TARGET}" "${APP_BACKUP_PATH}"
# This is the exact observable state if a signal arrives immediately after the
# first rename and before the shell can execute another assignment.
rollback_app_bundle_swap
[ "$(/bin/cat "${INTERRUPT_TARGET}/version")" = old ] || fail 'interrupt-boundary rollback lost the old app'
cleanup_app_bundle_swap_paths

reset_state
FAIL_INSTALL_DIR="${TEMP_ROOT}/FailureApplications"
FAIL_TARGET="${FAIL_INSTALL_DIR}/${BUNDLE_NAME}"
/bin/mkdir -p "${FAIL_TARGET}"
/usr/bin/printf 'old\n' > "${FAIL_TARGET}/version"
prepare_app_bundle_staging "${SOURCE}" "${FAIL_INSTALL_DIR}" "${BUNDLE_NAME}"
activate_app_bundle_staging "${FAIL_TARGET}"
PRESERVED_BACKUP="${APP_BACKUP_PATH}"
APP_TARGET_PATH="${TEMP_ROOT}/missing-restore-parent/${BUNDLE_NAME}"
if rollback_app_bundle_swap 2>/dev/null; then
    fail 'rollback unexpectedly accepted an unavailable restore path'
fi
cleanup_app_bundle_swap_paths
[ -d "${PRESERVED_BACKUP}" ] || fail 'cleanup erased the last backup after rollback failure'

reset_state
prepare_app_bundle_staging "${SOURCE}" "${INSTALL_DIR}" "${BUNDLE_NAME}"
activate_app_bundle_staging "${TARGET}"
[ "$(/bin/cat "${TARGET}/version")" = new ] || fail 'staged app was not activated'
rollback_app_bundle_swap
[ "$(/bin/cat "${TARGET}/version")" = old ] || fail 'rollback did not restore the old app'
cleanup_app_bundle_swap_paths

reset_state
prepare_app_bundle_staging "${SOURCE}" "${INSTALL_DIR}" "${BUNDLE_NAME}"
activate_app_bundle_staging "${TARGET}"
COMMITTED_BACKUP="${APP_BACKUP_PATH}"
commit_app_bundle_swap
[ "${APP_SWAP_ACTIVE}" -eq 0 ] || fail 'commit left rollback armed during backup cleanup'
[ -d "${COMMITTED_BACKUP}" ] || fail 'commit deleted the backup before disarming rollback'
# This is the state observed by EXIT if cleanup is interrupted or fails. A
# rollback must be a no-op so the working new app remains authoritative.
rollback_app_bundle_swap
[ "$(/bin/cat "${TARGET}/version")" = new ] || fail 'post-commit cleanup could roll back the new app'
cleanup_app_bundle_swap_paths
[ "$(/bin/cat "${TARGET}/version")" = new ] || fail 'committed app did not remain installed'

printf 'transactional app bundle swap: ok\n'
