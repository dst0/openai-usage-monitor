#!/bin/bash

# Transactional helpers sourced by install.sh. All paths are allocated beneath
# the destination filesystem so activation and rollback use rename operations.

prepare_app_bundle_staging() {
    local source_bundle="$1"
    local install_dir="$2"
    local bundle_name="$3"

    APP_STAGE_ROOT="$(/usr/bin/mktemp -d "${install_dir}/.codex-monitor-install.XXXXXX")" || return 1
    APP_STAGING_PATH="${APP_STAGE_ROOT}/${bundle_name}"
    if ! /bin/cp -R "${source_bundle}" "${APP_STAGING_PATH}"; then
        /bin/rm -rf -- "${APP_STAGE_ROOT}"
        APP_STAGE_ROOT=""
        APP_STAGING_PATH=""
        return 1
    fi
    /bin/chmod -R 755 "${APP_STAGING_PATH}" || return 1
    APP_BACKUP_ROOT="$(/usr/bin/mktemp -d "${install_dir}/.codex-monitor-backup.XXXXXX")" || return 1
    APP_BACKUP_PATH="${APP_BACKUP_ROOT}/${bundle_name}"
}

activate_app_bundle_staging() {
    local target="$1"
    if [ -z "${APP_STAGING_PATH}" ] || [ ! -d "${APP_STAGING_PATH}" ]; then
        echo "❌ Refusing installation: staged Monitor app is missing."
        return 1
    fi
    if [ -L "${target}" ] || { [ -e "${target}" ] && [ ! -d "${target}" ]; }; then
        echo "❌ Refusing installation: installed Monitor path is not a regular app directory."
        return 1
    fi

    APP_TARGET_PATH="${target}"
    if [ -d "${target}" ]; then
        APP_HAD_EXISTING_TARGET=1
        APP_SWAP_ACTIVE=1
        /bin/mv "${target}" "${APP_BACKUP_PATH}" || return 1
    else
        APP_HAD_EXISTING_TARGET=0
        APP_SWAP_ACTIVE=1
    fi
    if ! /bin/mv "${APP_STAGING_PATH}" "${target}"; then
        rollback_app_bundle_swap
        return 1
    fi
    APP_STAGING_PATH=""
}

rollback_app_bundle_swap() {
    [ "${APP_SWAP_ACTIVE}" -eq 1 ] || return 0
    if [ -n "${APP_BACKUP_PATH}" ] && [ -d "${APP_BACKUP_PATH}" ]; then
        if [ -n "${APP_TARGET_PATH}" ] && { [ -e "${APP_TARGET_PATH}" ] || [ -L "${APP_TARGET_PATH}" ]; }; then
            /bin/rm -rf -- "${APP_TARGET_PATH}" || return 1
        fi
        /bin/mv "${APP_BACKUP_PATH}" "${APP_TARGET_PATH}" || return 1
    elif [ "${APP_HAD_EXISTING_TARGET}" -eq 0 ] && [ -n "${APP_TARGET_PATH}" ] &&
        { [ -e "${APP_TARGET_PATH}" ] || [ -L "${APP_TARGET_PATH}" ]; }; then
        /bin/rm -rf -- "${APP_TARGET_PATH}" || return 1
    elif [ "${APP_HAD_EXISTING_TARGET}" -eq 1 ] && [ ! -d "${APP_TARGET_PATH}" ]; then
        return 1
    fi
    APP_HAD_EXISTING_TARGET=0
    APP_SWAP_ACTIVE=0
}

commit_app_bundle_swap() {
    [ "${APP_SWAP_ACTIVE}" -eq 1 ] || return 0
    # Commit is the irreversible rollback boundary. Disarm before any
    # best-effort backup cleanup: an interrupted or partial deletion must never
    # make EXIT restore an incomplete backup over the working new bundle.
    APP_HAD_EXISTING_TARGET=0
    APP_SWAP_ACTIVE=0
}

cleanup_app_bundle_swap_paths() {
    if [ -n "${APP_STAGE_ROOT}" ] && [ -d "${APP_STAGE_ROOT}" ]; then
        /bin/rm -rf -- "${APP_STAGE_ROOT}"
    fi
    # A failed rollback leaves APP_SWAP_ACTIVE armed. Preserve its backup as
    # the last recoverable copy instead of erasing evidence/state on EXIT. A
    # committed swap is disarmed before this best-effort cleanup runs.
    if [ "${APP_SWAP_ACTIVE}" -eq 0 ] && [ -n "${APP_BACKUP_ROOT}" ] && [ -d "${APP_BACKUP_ROOT}" ]; then
        /bin/rm -rf -- "${APP_BACKUP_ROOT}"
    fi
}
