#!/usr/bin/env bash
set -u
set -o pipefail

# OpenAI Codex Monitor & Switcher — conservative macOS uninstaller.
#
# By default this command prints the removal plan and requires the operator to
# type REMOVE. Use --yes only after reviewing that plan. CODEX_HOME may be set
# when the switcher was deliberately configured to use a non-default Codex
# home.
#
# This removes this project's installed app, helper binaries, notifier, launch
# items, app-owned state/logs, and the exact shell/skill registrations written
# by scripts/install.sh. While an installer holds the install lock, a
# confirmed run stops before changing anything; staging, backup, and clone
# leftovers of a killed install are removed only while no installer holds
# it. It deliberately preserves ChatGPT.app and Codex data
# shared with it: auth.json, state_5.sqlite, sessions/, thread-writer-locks/,
# and other transcript/database files. Source checkouts and build directories
# are left untouched.

APP_NAME="Codex Monitor"
NOTIFIER_NAME="Codex Notifier"
DAEMON_LABEL="com.codex.switcher"
RESTART_WORKER_LABEL="com.codex.switcher.restart-worker"
APP_SERVICE_LABEL="com.codex.monitor"
NOTIFIER_BUNDLE_ID="com.codex.switcher.notifier"
APP_SERVICE_LABEL_PREFIX="application.com.codex.monitor."
NOTIFIER_SERVICE_LABEL_PREFIX="application.${NOTIFIER_BUNDLE_ID}."

ASSUME_YES=0
DRY_RUN=0
PURGE_DATA=0
FAILED=0
CURRENT_UID="$(/usr/bin/id -u 2>/dev/null || true)"
USER_HOME="${HOME:-}"

usage() {
    cat <<'EOF'
Usage: scripts/uninstall.sh [--yes] [--dry-run] [--purge-data]

Remove the installed OpenAI Codex Monitor & Switcher components from this
macOS user account. Without --yes, the command shows the plan and requires
typing REMOVE. --yes is the explicit non-interactive confirmation.
Use --dry-run to print the same plan without changing anything.
Use --purge-data to additionally remove the Monitor-owned account registry
under CODEX_HOME; it is intentionally separate because accounts.json contains
stored account credentials.

The uninstaller does not remove ChatGPT.app or shared Codex credentials,
threads, transcripts, or databases. Set CODEX_HOME only when the monitor was
installed against a deliberate alternate Codex home.
EOF
}

die() {
    printf 'Error: %s\n' "$*"
    exit 1
}

warn() {
    printf 'Warning: %s\n' "$*"
}

note() {
    printf '%s\n' "$*"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --yes|-y) ASSUME_YES=1 ;;
        --dry-run) DRY_RUN=1 ;;
        --purge-data) PURGE_DATA=1 ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage >&2
            die "unknown option: $1"
            ;;
    esac
    shift
done

[ "$(/usr/bin/uname -s 2>/dev/null || true)" = "Darwin" ] ||
    die "this uninstaller is for macOS only"
[ -n "$USER_HOME" ] || die 'HOME is not set'
[ -n "$CURRENT_UID" ] || die 'cannot determine the current user id'
case "$USER_HOME" in
    /*) ;;
    *) die "HOME must be an absolute path" ;;
esac

# Resolve HOME only for safety checks. Keep the lexical HOME path because that
# is what install.sh recorded in LaunchAgents and ~/.local/bin.
HOME_DIR="$(cd "$USER_HOME" 2>/dev/null && /bin/pwd -P)" ||
    die "cannot resolve HOME: $USER_HOME"
[ "$HOME_DIR" != "/" ] || die "refusing to operate with HOME=/"

CODEX_HOME="${CODEX_HOME:-${USER_HOME}/.codex}"
case "$CODEX_HOME" in
    /*) ;;
    *) die "CODEX_HOME must be an absolute path" ;;
esac
case "$CODEX_HOME" in
    *..*) die "CODEX_HOME must not contain '..' path components" ;;
esac
if [ -L "$CODEX_HOME" ]; then
    die 'refusing to operate through a symlinked CODEX_HOME'
fi
if [ -e "$CODEX_HOME" ] && [ ! -d "$CODEX_HOME" ]; then
    die 'refusing to operate when CODEX_HOME is not a directory'
fi
if [ -d "$CODEX_HOME" ]; then
    CODEX_HOME="$(cd "$CODEX_HOME" 2>/dev/null && /bin/pwd -P)" ||
        die "cannot resolve CODEX_HOME"
fi
if [ "$CODEX_HOME" = "/" ] || [ "$CODEX_HOME" = "$USER_HOME" ] ||
   [ "$CODEX_HOME" = "$HOME_DIR" ]; then
    die "refusing to treat HOME or the filesystem root as CODEX_HOME"
fi

LOCAL_BIN="${USER_HOME}/.local/bin"
MONITOR_LOG_HELPER="${LOCAL_BIN}/codex-mon"
LAUNCH_AGENTS="${USER_HOME}/Library/LaunchAgents"
TMP_ROOT="${TMPDIR:-/tmp}"
case "$TMP_ROOT" in
    /*) ;;
    *) die "TMPDIR must be an absolute path" ;;
esac
[ "$TMP_ROOT" != "/" ] || die "refusing to use the filesystem root as TMPDIR"
DAEMON_PLIST="${LAUNCH_AGENTS}/${DAEMON_LABEL}.plist"
APP_SERVICE_PLIST="${LAUNCH_AGENTS}/${APP_SERVICE_LABEL}.plist"
# install.sh uses the first when it is writable, otherwise the second.
APPLICATION_DIRS=(
    "/Applications"
    "${USER_HOME}/Applications"
)
APP_PATHS=(
    "/Applications/${APP_NAME}.app"
    "${USER_HOME}/Applications/${APP_NAME}.app"
)
INSTALLED_HELPERS=(
    "${LOCAL_BIN}/codex-ui-resume"
    "${LOCAL_BIN}/codex-recovery-banner"
    "${LOCAL_BIN}/codex-window-restore"
)
NOTIFIER_PATH="${USER_HOME}/Applications/${NOTIFIER_NAME}.app"

ALWAYS_STATE_PATHS=(
    "${CODEX_HOME}/helps.html"
    "${CODEX_HOME}/usage-status.json"
    "${CODEX_HOME}/daemon.lock"
    "${CODEX_HOME}/codex.lock"
    "${CODEX_HOME}/monitor.lock"
    "${CODEX_HOME}/desktop-recovery.lock"
    "${CODEX_HOME}/desktop-automation-cooldown"
    "${CODEX_HOME}/desktop-recovery.json"
    "${CODEX_HOME}/desktop-app-session.json"
    "${CODEX_HOME}/distribution-journal.json"
    "${CODEX_HOME}/desktop-window.json"
    "${CODEX_HOME}/desktop-app-session.json"
    "${CODEX_HOME}/distribution-journal.json"
    "${CODEX_HOME}/direct-switch-journal.json"
    "${CODEX_HOME}/auto-reset-state.json"
    "${CODEX_HOME}/manual-reset-state.json"
)

# Interrupted atomic writes can leave private staging files. Match only the
# exact Monitor-generated names and modes; preserve unrelated files. Keep this
# list in step with every Rust staging writer that the fd-anchored
# `codex-mon monitor-logs` cleanup does not already cover:
#   distribution_journal.rs          distribution-journal.<pid>.<16 hex>.tmp
#   direct_switch_journal_store.rs   direct-switch-journal.<pid>.<16 hex>.tmp
#   desktop_app_session.rs           desktop-app-session.<pid>.<32 hex>.tmp
#   manual_reset_attempt_store.rs    manual-reset-state.<pid>.<16 hex>.tmp.json
#   active_auth_compare_write_service.rs
#                                    auth.json.<pid>.<16 hex>.tmp
# The last one is a Monitor-written credential copy, not Desktop's auth.json.
monitor_state_temps() {
    local path name metadata
    for path in \
        "${CODEX_HOME}"/distribution-journal.*.tmp \
        "${CODEX_HOME}"/direct-switch-journal.*.tmp \
        "${CODEX_HOME}"/desktop-app-session.*.tmp \
        "${CODEX_HOME}"/manual-reset-state.*.tmp.json \
        "${CODEX_HOME}"/auth.json.*.tmp; do
        is_present "$path" || continue
        [ ! -L "$path" ] && [ -f "$path" ] || continue
        name="${path##*/}"
        case "$name" in
            distribution-journal.*)
                [[ "$name" =~ ^distribution-journal\.[0-9]+\.[0-9a-f]{16}\.tmp$ ]] || continue ;;
            direct-switch-journal.*)
                [[ "$name" =~ ^direct-switch-journal\.[0-9]+\.[0-9a-f]{16}\.tmp$ ]] || continue ;;
            desktop-app-session.*)
                [[ "$name" =~ ^desktop-app-session\.[0-9]+\.[0-9a-f]{32}\.tmp$ ]] || continue ;;
            manual-reset-state.*)
                [[ "$name" =~ ^manual-reset-state\.[0-9]+\.[0-9a-f]{16}\.tmp\.json$ ]] || continue ;;
            auth.json.*)
                [[ "$name" =~ ^auth\.json\.[0-9]+\.[0-9a-f]{16}\.tmp$ ]] || continue ;;
            *) continue ;;
        esac
        metadata="$(/usr/bin/stat -f '%u:%Lp' "$path" 2>/dev/null || true)"
        [ "$metadata" = "${CURRENT_UID}:600" ] || continue
        printf '%s\n' "$path"
    done
}

# One character that macOS mktemp(1) substitutes for an X. Listed rather than
# written as ranges, so no locale's collation can widen the match.
MKTEMP_CHAR='[0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz]'
CLI_STAGING_NAME="^\\.codex-mon\\.install\\.${MKTEMP_CHAR}{6}(\\.cstemp)?\$"
BUNDLE_STAGING_NAME="^\\.codex-monitor-(install|backup)\\.${MKTEMP_CHAR}{6}\$"
# `mktemp -t` appends eight characters on macOS 13 and ten from macOS 14.
REMOTE_CLONE_NAME="^codex-mon-install-XXXXXX\\.(${MKTEMP_CHAR}{8}|${MKTEMP_CHAR}{10})\$"
UNINSTALL_COPY_NAME="^(\\.zshrc|\\.bash_profile|config\\.toml)\\.codex-monitor-uninstall\\.${MKTEMP_CHAR}{6}\$"
INSTALL_LOCK_HELD='an installer holds the install lock'
INSTALL_LOCK_UNVERIFIED='the install lock cannot be verified'
TEMP_DIR_UNKNOWN='the per-user temporary directory is unknown'

# The per-user temporary directory that `mktemp -t` uses, without its
# trailing slash. Fails unless getconf names an absolute directory.
per_user_temp_dir() {
    local dir
    dir="$(/usr/bin/getconf DARWIN_USER_TEMP_DIR 2>/dev/null)" || return 1
    dir="${dir%/}"
    case "$dir" in
        /?*) printf '%s\n' "$dir" ;;
        *) return 1 ;;
    esac
}

# Every file an installer may hold as its install lock, each once. install.sh
# locks one in the per-user temporary directory whatever TMPDIR says; earlier
# installers locked ${TMPDIR:-/tmp}'s, so this TMPDIR's and /tmp's are
# checked too. PER_USER_TEMP_DIR is empty when that directory is unknown,
# which leaves the install lock unverifiable.
INSTALL_LOCK_NAME="codex_monitor_install_${CURRENT_UID}.lock"
INSTALL_LOCK_FILES=()
add_install_lock_dir() {
    local dir="$1" path existing
    while [ "${dir%/}" != "$dir" ]; do dir="${dir%/}"; done
    path="${dir}/${INSTALL_LOCK_NAME}"
    for existing in ${INSTALL_LOCK_FILES[@]+"${INSTALL_LOCK_FILES[@]}"}; do
        [ "$existing" != "$path" ] || return 0
    done
    INSTALL_LOCK_FILES+=("$path")
}
PER_USER_TEMP_DIR="$(per_user_temp_dir)" || PER_USER_TEMP_DIR=""
[ -z "$PER_USER_TEMP_DIR" ] || add_install_lock_dir "$PER_USER_TEMP_DIR"
add_install_lock_dir "$TMP_ROOT"
add_install_lock_dir "/tmp"

private_directory() {
    [ ! -L "$1" ] && [ -d "$1" ] &&
        [ "$(/usr/bin/stat -f '%u:%Lp' "$1" 2>/dev/null || true)" = "${CURRENT_UID}:700" ]
}

# An app staging or backup root holds nothing, or only the Monitor bundle as
# a real directory.
holds_only_monitor_bundle() {
    local entry
    for entry in "$1"/* "$1"/.*; do
        case "${entry##*/}" in .|..) continue ;; esac
        is_present "$entry" || continue
        [ "${entry##*/}" = "${APP_NAME}.app" ] && [ ! -L "$entry" ] && [ -d "$entry" ] || return 1
    done
}

# Leftovers of a killed install. install.sh creates these only while it holds
# the install lock, and its EXIT cleanup removes them before releasing it.
# macOS mktemp(1) fills each X from MKTEMP_CHAR; `mktemp -t` keeps the Xs of
# its prefix and appends a dot and ten such characters.
#   ~/.local/bin/.codex-mon.install.XXXXXX         CLI staging (0600, 0755 after chmod)
#   ~/.local/bin/.codex-mon.install.XXXXXX.cstemp  codesign's copy while signing (0755 only)
#   APPLICATION_DIRS/.codex-monitor-install.XXXXXX app staging root (0700)
#   APPLICATION_DIRS/.codex-monitor-backup.XXXXXX  prior-app backup root (0700)
#   <per-user temp dir>/codex-mon-install-XXXXXX.XXXXXXXXXX
#                                                  remote-install clone (0700)
# A staging or backup root may hold only the Monitor bundle. While an install
# runs, a backup root can hold the only copy of the previous app, so callers
# remove these only when no installer holds the install lock.
installer_temps() {
    local path name metadata dir temp_dir
    for path in "${LOCAL_BIN}"/.codex-mon.install.*; do
        [ ! -L "$path" ] && [ -f "$path" ] || continue
        name="${path##*/}"
        [[ "$name" =~ $CLI_STAGING_NAME ]] || continue
        # codesign creates its copy with the mode of the file it signs, which
        # install.sh has already made 0755.
        metadata="$(/usr/bin/stat -f '%u:%Lp' "$path" 2>/dev/null || true)"
        case "$metadata" in
            "${CURRENT_UID}:755") ;;
            "${CURRENT_UID}:600") [[ "$name" != *.cstemp ]] || continue ;;
            *) continue ;;
        esac
        printf '%s\n' "$path"
    done
    for dir in "${APPLICATION_DIRS[@]}"; do
        for path in "$dir"/.codex-monitor-install.* "$dir"/.codex-monitor-backup.*; do
            name="${path##*/}"
            [[ "$name" =~ $BUNDLE_STAGING_NAME ]] || continue
            private_directory "$path" && holds_only_monitor_bundle "$path" || continue
            printf '%s\n' "$path"
        done
    done
    temp_dir="$PER_USER_TEMP_DIR"
    [ -n "$temp_dir" ] || return 0
    for path in "$temp_dir"/codex-mon-install-XXXXXX.*; do
        name="${path##*/}"
        [[ "$name" =~ $REMOTE_CLONE_NAME ]] || continue
        private_directory "$path" || continue
        printf '%s\n' "$path"
    done
}

# Tries once to take the exclusive flock(2) lock of the file open as
# descriptor $1. Exit status 0: taken (or already held through this
# descriptor); 75 (EX_TEMPFAIL): another open file holds it; anything else:
# unknown. /usr/bin/lockf, and its descriptor form, first ship with macOS 15;
# macOS 13 and 14 have perl's flock, the same lock. A lockf without the
# descriptor form rejects it as a usage error (64), which falls back too.
# Callers wait by retrying: lockf's own wait on a descriptor spins a CPU.
# scripts/install.sh keeps an identical copy of this function.
flock_fd_now() {
    local fd="$1"
    local status=69
    if [ -x /usr/bin/lockf ]; then
        /usr/bin/lockf -s -t 0 "${fd}" 2>/dev/null && return 0 || status=$?
        [ "${status}" -eq 64 ] || return "${status}"
    fi
    [ -x /usr/bin/perl ] || return 69
    /usr/bin/perl -MErrno -MFcntl=:flock -e '
        open(my $lock, "<&=", $ARGV[0]) or exit 71;
        flock($lock, LOCK_EX | LOCK_NB) or exit($!{EWOULDBLOCK} ? 75 : 71);
        exit 0;
    ' "${fd}" 2>/dev/null
}

# Probes the exclusive flock(2) lock of an existing regular file without
# waiting, through descriptor 8, and releases it; with `unlink`, removes the
# file while still holding the lock. The file is opened only for reading, so
# a probe never creates it. A path that no longer names the locked file was
# replaced or removed meanwhile (an installer that locked a removed lock file
# locks the new one instead), so the probe starts over, a bounded number of
# times. Exit status 0: free (and removed with `unlink`); 66: the path does
# not exist; 74: the free file could not be removed; 75 (EX_TEMPFAIL):
# another process holds it; anything else, including flock_fd_now's 69 for
# no lock tool: unknown.
try_flock() {
    local path="$1"
    local action="${2:-keep}"
    local attempt status opened
    for attempt in 1 2 3 4 5; do
        is_present "$path" || return 66
        [ ! -L "$path" ] && [ -f "$path" ] || return 71
        if ! { exec 8<"$path"; } 2>/dev/null; then
            is_present "$path" || return 66
            return 71
        fi
        flock_fd_now 8
        status=$?
        if [ "$status" -eq 0 ]; then
            opened="$(/usr/bin/stat -f '%d:%i' 0<&8 2>/dev/null)"
            if [ -z "$opened" ] || [ "$opened" != "$(/usr/bin/stat -f '%d:%i' "$path" 2>/dev/null)" ]; then
                exec 8<&-
                continue
            fi
            if [ "$action" = unlink ]; then
                /bin/rm -f -- "$path" 2>/dev/null || status=74
            fi
        fi
        exec 8<&-
        return "$status"
    done
    return 71
}

# Prints nothing when no installer holds an install lock (or none exists),
# otherwise why an installer may still be running. The probe keeps each lock
# file and never creates one, so a dry run changes nothing. A held lock wins
# over one that cannot be verified: a lock file that is a symlink, not a
# regular file, another user's, or unreadable, or an unknown per-user
# temporary directory.
install_lock_blocker() {
    local path status
    local verdict=""
    [ -n "$PER_USER_TEMP_DIR" ] || verdict="$INSTALL_LOCK_UNVERIFIED"
    for path in "${INSTALL_LOCK_FILES[@]}"; do
        is_present "$path" || continue
        if [ -L "$path" ] || [ ! -f "$path" ] ||
           [ "$(/usr/bin/stat -f '%u' "$path" 2>/dev/null || true)" != "$CURRENT_UID" ]; then
            verdict="$INSTALL_LOCK_UNVERIFIED"
            continue
        fi
        try_flock "$path"
        status=$?
        case "$status" in
            0|66) ;;
            75)
                printf '%s\n' "$INSTALL_LOCK_HELD"
                return 0
                ;;
            *) verdict="$INSTALL_LOCK_UNVERIFIED" ;;
        esac
    done
    [ -z "$verdict" ] || printf '%s\n' "$verdict"
}

# Private copies an interrupted uninstall leaves while clean_rc_file or
# clean_codex_skill_config edits a file. A copy takes the edited file's mode,
# so only its owner, type, and exact name are checked.
uninstaller_temps() {
    local path name
    for path in \
        "${USER_HOME}"/.zshrc.codex-monitor-uninstall.* \
        "${USER_HOME}"/.bash_profile.codex-monitor-uninstall.* \
        "${USER_HOME}"/.codex/config.toml.codex-monitor-uninstall.*; do
        [ ! -L "$path" ] && [ -f "$path" ] || continue
        name="${path##*/}"
        [[ "$name" =~ $UNINSTALL_COPY_NAME ]] || continue
        [ "$(/usr/bin/stat -f '%u' "$path" 2>/dev/null || true)" = "$CURRENT_UID" ] || continue
        printf '%s\n' "$path"
    done
}

# Exact bundle-specific Library paths only; never remove a broad Library tree.
ALWAYS_ARTIFACT_PATHS=(
    "${USER_HOME}/Library/Preferences/com.codex.monitor.plist"
    "${USER_HOME}/Library/Preferences/com.dst.codex-monitor.plist"
    "${USER_HOME}/Library/Preferences/${NOTIFIER_BUNDLE_ID}.plist"
    "${USER_HOME}/Library/Logs/${APP_NAME}"
    "${USER_HOME}/Library/Logs/${NOTIFIER_NAME}"
    "${USER_HOME}/.local/share/codex-monitor"
    "${USER_HOME}/Library/Application Support/${APP_NAME}"
    "${USER_HOME}/Library/Application Support/${NOTIFIER_NAME}"
    "${USER_HOME}/Library/Caches/com.codex.monitor"
    "${USER_HOME}/Library/Caches/${NOTIFIER_BUNDLE_ID}"
    "${USER_HOME}/Library/Saved Application State/com.codex.monitor.savedState"
    "${USER_HOME}/Library/Saved Application State/${NOTIFIER_BUNDLE_ID}.savedState"
    "${USER_HOME}/Library/WebKit/com.codex.monitor"
    "${USER_HOME}/Library/HTTPStorages/com.codex.monitor"
)

is_present() {
    [ -e "$1" ] || [ -L "$1" ]
}

monitor_log_helper_available() {
    [ -f "$MONITOR_LOG_HELPER" ] && [ ! -L "$MONITOR_LOG_HELPER" ] && [ -x "$MONITOR_LOG_HELPER" ]
}

print_monitor_log_plan() {
    if monitor_log_helper_available; then
        local args=(monitor-logs --dry-run)
        [ "$PURGE_DATA" -eq 1 ] && args+=(--purge-data)
        if ! CODEX_HOME="$CODEX_HOME" "$MONITOR_LOG_HELPER" "${args[@]}"; then
            note '  preserve Monitor log/recovery cleanup (unsafe path or helper failure)'
        fi
    else
        note '  preserve Monitor log/recovery cleanup (fd-anchored helper unavailable)'
    fi
}

run_monitor_log_helper() {
    local action="$1"
    if ! monitor_log_helper_available; then
        warn 'fd-anchored Monitor log helper is unavailable; preserving Monitor log state'
        FAILED=1
        return 1
    fi
    local args=(monitor-logs "$action")
    [ "$PURGE_DATA" -eq 1 ] && args+=(--purge-data)
    if ! CODEX_HOME="$CODEX_HOME" "$MONITOR_LOG_HELPER" "${args[@]}" >/dev/null; then
        warn 'fd-anchored Monitor log cleanup refused an unsafe path'
        FAILED=1
        return 1
    fi
}

print_plan_path() {
    is_present "$1" && printf '  remove %s\n' "$1"
}

print_installer_temp_plan() {
    local blocker="$1"
    local path
    while IFS= read -r path; do
        if [ -n "$blocker" ]; then
            printf '  preserve %s (%s)\n' "$path" "$blocker"
        else
            printf '  remove %s\n' "$path"
        fi
    done < <(installer_temps)
    [ -n "$PER_USER_TEMP_DIR" ] ||
        printf '  preserve remote-install clones (%s)\n' "$TEMP_DIR_UNKNOWN"
}

print_plan() {
    local blocker
    blocker="$(install_lock_blocker)"
    if [ "$blocker" = "$INSTALL_LOCK_HELD" ]; then
        note "${INSTALL_LOCK_HELD}: a confirmed uninstall stops without changing anything until the installation ends."
    fi
    note "This will remove installed ${APP_NAME} components for user ${USER_HOME}:"
    for path in "${APP_PATHS[@]}"; do print_plan_path "$path"; done
    print_plan_path "$NOTIFIER_PATH"
    print_plan_path "$DAEMON_PLIST"
    print_plan_path "$APP_SERVICE_PLIST"
    for path in "${INSTALL_LOCK_FILES[@]}"; do print_plan_path "$path"; done
    for path in "${ALWAYS_STATE_PATHS[@]}"; do print_plan_path "$path"; done
    while IFS= read -r path; do print_plan_path "$path"; done < <(monitor_state_temps)
    print_installer_temp_plan "$blocker"
    while IFS= read -r path; do print_plan_path "$path"; done < <(uninstaller_temps)
    print_monitor_log_plan
    for path in "${ALWAYS_ARTIFACT_PATHS[@]}"; do print_plan_path "$path"; done
    if [ "$PURGE_DATA" -eq 0 ]; then
        note "  preserve Monitor account registry (use --purge-data to remove stored account copies)"
    fi
    note "  remove app-owned ~/.local/bin binaries/shims and skill symlinks when their targets identify this project"
    note "  remove exact Codex Monitor PATH lines and exact skill blocks added by the installer"
    note "  unregister the two app bundles and their login/service entries"
    note ""
    note "Protected (not removed):"
    if [ "$PURGE_DATA" -eq 0 ]; then
        note "  ${CODEX_HOME}/accounts.json (use --purge-data to remove this stored account registry)"
    fi
    note "  ${CODEX_HOME}/auth.json"
    note "  ${CODEX_HOME}/ipc/ and ${CODEX_HOME}/app-server-daemon/"
    note "  ${CODEX_HOME}/state_5.sqlite and its -wal/-shm files"
    note "  ${CODEX_HOME}/sessions/ and ${CODEX_HOME}/thread-writer-locks/"
    note "  ${USER_HOME}/Library/Application Support/ChatGPT and /Applications/ChatGPT.app"
    note "  this repository checkout and its build directories"
}

if [ "$DRY_RUN" -eq 1 ]; then
    print_plan
    note 'Dry run: no changes made.'
    exit 0
fi

if [ "$ASSUME_YES" -eq 0 ]; then
    print_plan
    [ -t 0 ] ||
        die 'stdin is not a terminal; review the plan and rerun with --yes for non-interactive use'
    printf 'Type REMOVE to continue: '
    IFS= read -r confirmation || confirmation=""
    [ "$confirmation" = "REMOVE" ] || {
        note "Uninstall cancelled."
        exit 0
    }
fi

remove_path() {
    local path="$1"
    is_present "$path" || return 0
    if [ -L "$path" ]; then
        /bin/rm -f -- "$path" 2>/dev/null || { warn "could not remove symlink: $path"; FAILED=1; }
    elif [ -d "$path" ]; then
        /bin/rm -rf -- "$path" 2>/dev/null || { warn "could not remove directory: $path"; FAILED=1; }
    else
        /bin/rm -f -- "$path" 2>/dev/null || { warn "could not remove file: $path"; FAILED=1; }
    fi
}

# Removes a lock file only while holding its lock, so a process that later
# locks the removed file sees that its path no longer names it.
remove_unlocked_file() {
    local path="$1"
    is_present "$path" || return 0
    if [ -L "$path" ] || [ ! -f "$path" ]; then
        remove_path "$path"
        return
    fi
    local status
    try_flock "$path" unlink
    status=$?
    case "$status" in
        # Removed, or removed by someone else meanwhile.
        0|66) ;;
        74)
            warn "could not remove file: $path"
            FAILED=1
            ;;
        75)
            warn "lock is still held; preserving: $path"
            FAILED=1
            ;;
        *)
            warn "cannot tell whether the lock is held; preserving: $path"
            FAILED=1
            ;;
    esac
}

# Installers leave their lock file behind; remove each one this user owns.
remove_install_lock_files() {
    local path
    for path in "${INSTALL_LOCK_FILES[@]}"; do
        if [ -f "$path" ] && [ ! -L "$path" ] &&
           [ "$(/usr/bin/stat -f '%u' "$path" 2>/dev/null || true)" != "$CURRENT_UID" ]; then
            warn "preserving an install lock file this user does not own: $path"
            FAILED=1
            continue
        fi
        remove_unlocked_file "$path"
    done
}

arm_cancellation_marker() {
    if ! monitor_log_helper_available; then
        warn 'fd-anchored Monitor log helper is unavailable; could not arm restart cancellation'
        FAILED=1
        return
    fi
    if ! CODEX_HOME="$CODEX_HOME" "$MONITOR_LOG_HELPER" monitor-logs --cancel >/dev/null; then
        warn 'fd-anchored Monitor recovery state refused cancellation marker'
        FAILED=1
    fi
}

unload_label() {
    local label="$1"
    if [ ! -x /bin/launchctl ]; then
        warn "/bin/launchctl is unavailable; could not unload $label"
        FAILED=1
        return
    fi
    /bin/launchctl bootout "gui/${CURRENT_UID}/${label}" >/dev/null 2>&1 || true
    /bin/launchctl remove "$label" >/dev/null 2>&1 || true
}

unload_matching_labels() {
    local prefix="$1"
    local label
    [ -x /bin/launchctl ] || return 0
    while IFS= read -r label; do
        [ -n "$label" ] || continue
        unload_label "$label"
    done < <(/bin/launchctl list 2>/dev/null | /usr/bin/awk -v prefix="$prefix" 'index($3, prefix) == 1 { print $3 }')
}

unload_plist() {
    local plist="$1"
    local label="$2"
    if is_present "$plist"; then
        if [ -L "$plist" ]; then
            warn "preserving symlinked LaunchAgent plist: $plist"
            FAILED=1
            unload_label "$label"
            return
        fi
        if [ -x /bin/launchctl ]; then
            /bin/launchctl bootout "gui/${CURRENT_UID}" "$plist" >/dev/null 2>&1 || true
            /bin/launchctl unload "$plist" >/dev/null 2>&1 || true
        else
            warn "/bin/launchctl is unavailable; could not unload $label"
            FAILED=1
        fi
    fi
    unload_label "$label"
}

# Match literal command-line paths from ps and signal only this user's
# processes. No generic pkill is used, so ChatGPT.app/app-server is excluded.
processes_with_path() {
    local path="$1"
    local pid owner command
    while read -r pid owner command; do
        [ -n "$pid" ] || continue
        [ "$pid" = "$$" ] && continue
        [ "$owner" = "$CURRENT_UID" ] || continue
        case "$command" in
            *"$path"*) printf '%s\n' "$pid" ;;
        esac
    done < <(/bin/ps -axo pid=,uid=,command= 2>/dev/null || true)
}

stop_processes_with_path() {
    local path="$1"
    local pid
    local pids=()
    while IFS= read -r pid; do
        [ -n "$pid" ] && pids+=("$pid")
    done < <(processes_with_path "$path")
    [ "${#pids[@]}" -gt 0 ] || return 0
    note "Stopping app process(es) for $path"
    for pid in "${pids[@]}"; do /bin/kill -TERM "$pid" 2>/dev/null || true; done

    local attempt remaining
    for attempt in 1 2 3 4 5 6 7 8 9 10; do
        remaining=0
        for pid in "${pids[@]}"; do
            if /bin/kill -0 "$pid" 2>/dev/null; then remaining=1; break; fi
        done
        [ "$remaining" -eq 0 ] && return 0
        /bin/sleep 0.1
    done

    for pid in "${pids[@]}"; do
        if /bin/kill -0 "$pid" 2>/dev/null; then
            local current_command
            current_command="$(/bin/ps -p "$pid" -o command= 2>/dev/null || true)"
            case "$current_command" in
                *"$path"*) /bin/kill -KILL "$pid" 2>/dev/null || true ;;
                *) warn "process $pid changed identity; leaving it running"; FAILED=1 ;;
            esac
        fi
    done
}

remove_ours_symlink() {
    local path="$1"
    shift
    [ -L "$path" ] || return 0
    local target expected
    target="$(/usr/bin/readlink "$path" 2>/dev/null || true)"
    for expected in "$@"; do
        [ "$target" = "$expected" ] && { remove_path "$path"; return; }
    done
    warn "preserving symlink with an unrelated target: $path -> $target"
}

remove_project_binary() {
    local path="$1"
    is_present "$path" || return 0
    if [ "$path" = "${LOCAL_BIN}/codex-mon" ]; then
        # Inspect a static build string rather than executing an arbitrary
        # binary during uninstall. This identifies the project's release
        # binary while preserving an unrelated user-owned codex-mon command.
        if [ -f "$path" ] && [ ! -L "$path" ] &&
           /usr/bin/strings "$path" 2>/dev/null | /usr/bin/grep -F "OpenAI Codex Account Switcher & Quota Monitor" >/dev/null 2>&1; then
            remove_path "$path"
        else
            warn "preserving an unrecognized or non-regular codex-mon path: $path"
            FAILED=1
        fi
        return
    fi
    remove_path "$path"
}

remove_skill_link() {
    local path="$1"
    local skill="$2"
    [ -L "$path" ] || {
        if is_present "$path"; then warn "preserving non-symlink skill path: $path"; fi
        return
    }
    local target
    target="$(/usr/bin/readlink "$path" 2>/dev/null || true)"
    case "$target" in
        # Current local checkout, the persistent remote-install cache, and
        # temporary clone paths used by older one-line installs are the only
        # targets written by install.sh. Do not remove an unrelated skill
        # symlink merely because it has the same final directory name.
        */openai-usage-monitor/skills/"$skill"|\
        "${USER_HOME}/.local/share/codex-monitor/skills/${skill}"|\
        */codex-mon-install-*/skills/"$skill") remove_path "$path" ;;
        *) warn "preserving unrelated skill symlink: $path -> $target" ;;
    esac
}

clean_rc_file() {
    local path="$1"
    [ -f "$path" ] || return 0
    if [ -L "$path" ]; then
        warn "preserving symlinked shell config: $path"
        FAILED=1
        return
    fi
    local temporary
    temporary="$(/usr/bin/mktemp "${path}.codex-monitor-uninstall.XXXXXX" 2>/dev/null || true)"
    [ -n "$temporary" ] || { warn "could not prepare shell config cleanup: $path"; FAILED=1; return; }
    local mode
    mode="$(/usr/bin/stat -f '%Lp' "$path" 2>/dev/null || true)"
    /usr/bin/awk \
        -v marker='# OpenAI Codex Monitor CLI path' \
        -v export_line='export PATH="$HOME/.local/bin:$PATH"' \
        '{
            if ($0 == marker) {
                following = ""
                if ((getline following) > 0 && following == export_line) { next }
                print $0
                if (following != "") print following
                next
            }
            print
        }' "$path" > "$temporary" 2>/dev/null || {
            /bin/rm -f -- "$temporary" 2>/dev/null || true
            warn "could not inspect shell config: $path"
            FAILED=1
            return
        }
    if /usr/bin/cmp -s "$path" "$temporary"; then
        /bin/rm -f -- "$temporary" 2>/dev/null || true
        return
    fi
    [ -n "$mode" ] && /bin/chmod "$mode" "$temporary" 2>/dev/null || true
    /bin/mv -f -- "$temporary" "$path" 2>/dev/null || {
        /bin/rm -f -- "$temporary" 2>/dev/null || true
        warn "could not update shell config: $path"
        FAILED=1
    }
}

clean_codex_skill_config() {
    # install.sh registers skills in the user's default Codex config even when
    # a caller later uses CODEX_HOME for runtime state.
    local path="${USER_HOME}/.codex/config.toml"
    [ -f "$path" ] || return 0
    if [ -L "$path" ]; then
        warn "preserving symlinked Codex config: $path"
        FAILED=1
        return
    fi
    local skill temporary mode
    for skill in cxi codex-mon; do
        temporary="$(/usr/bin/mktemp "${path}.codex-monitor-uninstall.XXXXXX" 2>/dev/null || true)"
        [ -n "$temporary" ] || { warn "could not prepare Codex config cleanup: $path"; FAILED=1; return; }
        local path_line
        path_line="path = \"${USER_HOME}/.codex/skills/${skill}/SKILL.md\""
        /usr/bin/awk \
            -v header='[[skills.config]]' \
            -v path_line="$path_line" \
            -v enabled_line='enabled = true' \
            '{
                if ($0 == header) {
                    first = ""; second = ""
                    first_ok = (getline first > 0)
                    second_ok = (getline second > 0)
                    if (first_ok && second_ok && first == path_line && second == enabled_line) { next }
                    print $0
                    if (first_ok) print first
                    if (second_ok) print second
                    next
                }
                print
            }' "$path" > "$temporary" 2>/dev/null || {
            /bin/rm -f -- "$temporary" 2>/dev/null || true
            warn "could not inspect Codex config: $path"
            FAILED=1
            return
        }
        if /usr/bin/cmp -s "$path" "$temporary"; then
            /bin/rm -f -- "$temporary" 2>/dev/null || true
            continue
        fi
        mode="$(/usr/bin/stat -f '%Lp' "$path" 2>/dev/null || true)"
        [ -n "$mode" ] && /bin/chmod "$mode" "$temporary" 2>/dev/null || true
        /bin/mv -f -- "$temporary" "$path" 2>/dev/null || {
            /bin/rm -f -- "$temporary" 2>/dev/null || true
            warn "could not update Codex config: $path"
            FAILED=1
            return
        }
    done
}

unregister_bundle() {
    local path="$1"
    is_present "$path" || return 0
    local lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
    [ -x "$lsregister" ] && "$lsregister" -u "$path" >/dev/null 2>&1 || true
}

remove_installer_temps() {
    local path paths blocker
    if [ -z "$PER_USER_TEMP_DIR" ]; then
        warn "${TEMP_DIR_UNKNOWN}; remote-install clones were not checked"
        FAILED=1
    fi
    paths="$(installer_temps)"
    [ -n "$paths" ] || return 0
    # List before probing the lock: installers create these paths only while
    # holding it, so a lock found free afterwards proves that every listed
    # path belongs to an installer that has exited.
    blocker="$(install_lock_blocker)"
    while IFS= read -r path; do
        if [ -n "$blocker" ]; then
            warn "preserving installer staging because ${blocker}: $path"
            FAILED=1
        else
            remove_path "$path"
        fi
    done <<< "$paths"
}

# An installer that holds the install lock may be replacing or rolling back
# the app right now. Stop before changing anything. The per-leftover check in
# remove_installer_temps still covers an installer that starts after this.
if [ "$(install_lock_blocker)" = "$INSTALL_LOCK_HELD" ]; then
    die "${INSTALL_LOCK_HELD}; nothing was changed. Rerun the uninstaller after the installation ends."
fi

note "Stopping OpenAI Codex Monitor & Switcher..."
arm_cancellation_marker

unload_plist "$DAEMON_PLIST" "$DAEMON_LABEL"
unload_plist "$APP_SERVICE_PLIST" "$APP_SERVICE_LABEL"
unload_label "$RESTART_WORKER_LABEL"
unload_matching_labels "$APP_SERVICE_LABEL_PREFIX"
unload_matching_labels "$NOTIFIER_SERVICE_LABEL_PREFIX"

if [ -x /usr/bin/osascript ]; then
    # Match the exact bundle path written by install.sh, not an unrelated
    # login item that happens to share the display name.
    /usr/bin/osascript - "$USER_HOME" <<'APPLESCRIPT' >/dev/null 2>&1 || {
on run argv
    set userHome to item 1 of argv
    set expectedPaths to {"/Applications/Codex Monitor.app", "/Applications/Codex Monitor.app/", userHome & "/Applications/Codex Monitor.app", userHome & "/Applications/Codex Monitor.app/"}
    tell application "System Events"
        repeat with loginItem in (every login item)
            try
                set itemPath to POSIX path of (path of loginItem)
                if itemPath is in expectedPaths then
                    delete loginItem
                end if
            end try
        end repeat
    end tell
end run
APPLESCRIPT
        warn 'could not remove the Codex Monitor login item'
        FAILED=1
    }
else
    warn '/usr/bin/osascript is unavailable; could not remove the Codex Monitor login item'
    FAILED=1
fi

for path in \
    "${USER_HOME}/.local/bin/codex-mon" \
    "${USER_HOME}/.local/bin/cxi" \
    "${INSTALLED_HELPERS[@]}" \
    "/Applications/${APP_NAME}.app/Contents/MacOS/CodexMonitor" \
    "${USER_HOME}/Applications/${APP_NAME}.app/Contents/MacOS/CodexMonitor" \
    "${NOTIFIER_PATH}/Contents/MacOS/notify"; do
    stop_processes_with_path "$path"
done

# Remove logs only after the daemon, app, and helpers have stopped writing.
run_monitor_log_helper --remove

for path in "${APP_PATHS[@]}"; do unregister_bundle "$path"; done
unregister_bundle "$NOTIFIER_PATH"
for path in "${APP_PATHS[@]}"; do remove_path "$path"; done
remove_path "$NOTIFIER_PATH"
remove_installer_temps

remove_project_binary "${LOCAL_BIN}/codex-mon"
for path in "${INSTALLED_HELPERS[@]}"; do remove_path "$path"; done
remove_ours_symlink "${LOCAL_BIN}/cxi" "${LOCAL_BIN}/codex-mon" "codex-mon"
remove_ours_symlink "${LOCAL_BIN}/codex" "${LOCAL_BIN}/codex-mon" "codex-mon"

# install.sh never writes /usr/local/bin, but remove a legacy symlink only when
# its target proves it points at this user's known app binary.
remove_ours_symlink "/usr/local/bin/codex-mon" "${LOCAL_BIN}/codex-mon"
remove_ours_symlink "/usr/local/bin/cxi" "${LOCAL_BIN}/codex-mon" "${LOCAL_BIN}/cxi"
remove_ours_symlink "/usr/local/bin/codex" "${LOCAL_BIN}/codex-mon"
remove_ours_symlink "/usr/local/bin/codex-ui-resume" "${LOCAL_BIN}/codex-ui-resume"

for skill in cxi codex-mon; do
    remove_skill_link "${USER_HOME}/.claude/skills/${skill}" "$skill"
    remove_skill_link "${USER_HOME}/.codex/skills/${skill}" "$skill"
    remove_skill_link "${USER_HOME}/.agents/skills/${skill}" "$skill"
done

clean_rc_file "${USER_HOME}/.zshrc"
clean_rc_file "${USER_HOME}/.bash_profile"
clean_codex_skill_config
while IFS= read -r path; do remove_path "$path"; done < <(uninstaller_temps)

for path in "${ALWAYS_STATE_PATHS[@]}"; do
    case "$path" in
        "${CODEX_HOME}/daemon.lock"|"${CODEX_HOME}/codex.lock"|"${CODEX_HOME}/monitor.lock"|"${CODEX_HOME}/desktop-recovery.lock")
            remove_unlocked_file "$path"
            ;;
        *) remove_path "$path" ;;
    esac
done
while IFS= read -r path; do remove_path "$path"; done < <(monitor_state_temps)
# Clear the preference domains as well as their plist files. This is scoped to
# the two bundle identifiers owned by this project and does not touch ChatGPT.
if [ -x /usr/bin/defaults ]; then
    /usr/bin/defaults delete com.codex.monitor >/dev/null 2>&1 || true
    /usr/bin/defaults delete com.dst.codex-monitor >/dev/null 2>&1 || true
    /usr/bin/defaults delete "${NOTIFIER_BUNDLE_ID}" >/dev/null 2>&1 || true
fi
for path in "${ALWAYS_ARTIFACT_PATHS[@]}"; do remove_path "$path"; done

# Remove the installers' coordination files only after the app and workers
# have stopped, and each only while no installer holds it.
remove_install_lock_files

if [ "$FAILED" -ne 0 ]; then
    warn 'uninstall completed with warnings; inspect the paths reported above'
    exit 1
fi
note 'Codex Monitor & Switcher uninstall complete.'
