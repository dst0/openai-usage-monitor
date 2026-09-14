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
# by scripts/install.sh. It deliberately preserves ChatGPT.app and Codex data
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
if [ -d "$CODEX_HOME" ]; then
    CODEX_HOME="$(cd "$CODEX_HOME" 2>/dev/null && /bin/pwd -P)" ||
        die "cannot resolve CODEX_HOME"
fi
if [ "$CODEX_HOME" = "/" ] || [ "$CODEX_HOME" = "$USER_HOME" ] ||
   [ "$CODEX_HOME" = "$HOME_DIR" ]; then
    die "refusing to treat HOME or the filesystem root as CODEX_HOME"
fi

LOCAL_BIN="${USER_HOME}/.local/bin"
LAUNCH_AGENTS="${USER_HOME}/Library/LaunchAgents"
TMP_ROOT="${TMPDIR:-/tmp}"
case "$TMP_ROOT" in
    /*) ;;
    *) die "TMPDIR must be an absolute path" ;;
esac
[ "$TMP_ROOT" != "/" ] || die "refusing to use the filesystem root as TMPDIR"
INSTALL_LOCK_FILE="${TMP_ROOT}/codex_monitor_install_${CURRENT_UID}.lock"
DAEMON_PLIST="${LAUNCH_AGENTS}/${DAEMON_LABEL}.plist"
APP_SERVICE_PLIST="${LAUNCH_AGENTS}/${APP_SERVICE_LABEL}.plist"
RECOVERY_DIR="${CODEX_HOME}/recovery-runs"

APP_PATHS=(
    "/Applications/${APP_NAME}.app"
    "${USER_HOME}/Applications/${APP_NAME}.app"
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
    "${CODEX_HOME}/auto-reset-state.json"
    "${CODEX_HOME}/account-switcher-daemon.log"
    "${CODEX_HOME}/account-switcher-daemon.err"
    "${RECOVERY_DIR}/cancel-restart"
)

PURGE_STATE_PATHS=(
    "${CODEX_HOME}/accounts.json"
)

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

print_plan_path() {
    is_present "$1" && printf '  remove %s\n' "$1"
}

print_plan() {
    note "This will remove installed ${APP_NAME} components for user ${USER_HOME}:"
    for path in "${APP_PATHS[@]}"; do print_plan_path "$path"; done
    print_plan_path "$NOTIFIER_PATH"
    print_plan_path "$DAEMON_PLIST"
    print_plan_path "$APP_SERVICE_PLIST"
    print_plan_path "$INSTALL_LOCK_FILE"
    for path in "${ALWAYS_STATE_PATHS[@]}"; do print_plan_path "$path"; done
    for path in "${ALWAYS_ARTIFACT_PATHS[@]}"; do print_plan_path "$path"; done
    if [ "$PURGE_DATA" -eq 1 ]; then
        for path in "${PURGE_STATE_PATHS[@]}"; do print_plan_path "$path"; done
    else
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
    note "  this source checkout and its build directories"
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

remove_matching_files() {
    local directory="$1"
    shift
    [ -d "$directory" ] || return 0
    if [ -L "$directory" ]; then
        warn "preserving symlinked runtime directory: $directory"
        FAILED=1
        return
    fi
    local pattern path
    for pattern in "$@"; do
        while IFS= read -r -d '' path; do
            remove_path "$path"
        done < <(/usr/bin/find "$directory" -maxdepth 1 -type f -name "$pattern" -print0 2>/dev/null)
    done
}

remove_unlocked_file() {
    local path="$1"
    is_present "$path" || return 0
    if [ ! -f "$path" ]; then
        remove_path "$path"
        return
    fi
    if ! command -v lockf >/dev/null 2>&1; then
        warn "lockf is unavailable; preserving possible lock file: $path"
        FAILED=1
        return
    fi
    if ! lockf -t 0 "$path" /usr/bin/true >/dev/null 2>&1; then
        warn "lock is still held; preserving: $path"
        FAILED=1
        return
    fi
    remove_path "$path"
}

arm_cancellation_marker() {
    [ -d "$CODEX_HOME" ] || return 0
    if [ -L "$RECOVERY_DIR" ]; then
        warn "preserving symlinked recovery directory: $RECOVERY_DIR"
        FAILED=1
        return
    fi
    /bin/mkdir -p "$RECOVERY_DIR" 2>/dev/null || {
        warn "could not create recovery cancellation directory: $RECOVERY_DIR"
        FAILED=1
        return
    }
    local temporary="${RECOVERY_DIR}/.cancel-restart.uninstall.$$"
    (umask 077 && /usr/bin/printf 'cancel\n' > "$temporary") 2>/dev/null || {
        warn "could not arm restart cancellation marker"
        FAILED=1
        return
    }
    /bin/mv -f -- "$temporary" "${RECOVERY_DIR}/cancel-restart" 2>/dev/null || {
        warn "could not install restart cancellation marker"
        /bin/rm -f -- "$temporary" 2>/dev/null || true
        FAILED=1
    }
}

unload_label() {
    local label="$1"
    if ! command -v launchctl >/dev/null 2>&1; then
        warn "launchctl is unavailable; could not unload $label"
        FAILED=1
        return
    fi
    /bin/launchctl bootout "gui/${CURRENT_UID}/${label}" >/dev/null 2>&1 || true
    /bin/launchctl remove "$label" >/dev/null 2>&1 || true
}

unload_matching_labels() {
    local prefix="$1"
    local label
    command -v launchctl >/dev/null 2>&1 || return 0
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
        if command -v launchctl >/dev/null 2>&1; then
            /bin/launchctl bootout "gui/${CURRENT_UID}" "$plist" >/dev/null 2>&1 || true
            /bin/launchctl unload "$plist" >/dev/null 2>&1 || true
        else
            warn "launchctl is unavailable; could not unload $label"
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

note "Stopping OpenAI Codex Monitor & Switcher..."
arm_cancellation_marker

unload_plist "$DAEMON_PLIST" "$DAEMON_LABEL"
unload_plist "$APP_SERVICE_PLIST" "$APP_SERVICE_LABEL"
unload_label "$RESTART_WORKER_LABEL"
unload_matching_labels "$APP_SERVICE_LABEL_PREFIX"
unload_matching_labels "$NOTIFIER_SERVICE_LABEL_PREFIX"

if command -v osascript >/dev/null 2>&1; then
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
    warn 'osascript is unavailable; could not remove the Codex Monitor login item'
    FAILED=1
fi

for path in \
    "${USER_HOME}/.local/bin/codex-mon" \
    "${USER_HOME}/.local/bin/cxi" \
    "${USER_HOME}/.local/bin/codex-ui-resume" \
    "/Applications/${APP_NAME}.app/Contents/MacOS/CodexMonitor" \
    "${USER_HOME}/Applications/${APP_NAME}.app/Contents/MacOS/CodexMonitor" \
    "${NOTIFIER_PATH}/Contents/MacOS/notify"; do
    stop_processes_with_path "$path"
done

for path in "${APP_PATHS[@]}"; do unregister_bundle "$path"; done
unregister_bundle "$NOTIFIER_PATH"
for path in "${APP_PATHS[@]}"; do remove_path "$path"; done
remove_path "$NOTIFIER_PATH"

remove_project_binary "${LOCAL_BIN}/codex-mon"
remove_path "${LOCAL_BIN}/codex-ui-resume"
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

for path in "${ALWAYS_STATE_PATHS[@]}"; do
    case "$path" in
        "${CODEX_HOME}/daemon.lock"|"${CODEX_HOME}/codex.lock"|"${CODEX_HOME}/monitor.lock"|"${CODEX_HOME}/desktop-recovery.lock")
            remove_unlocked_file "$path"
            ;;
        *) remove_path "$path" ;;
    esac
done
if [ "$PURGE_DATA" -eq 1 ]; then
    for path in "${PURGE_STATE_PATHS[@]}"; do remove_path "$path"; done
fi
remove_matching_files "$CODEX_HOME" \
    'auth.*.tmp.json' 'accounts.*.tmp.json' 'usage-status.*.tmp.json' \
    'desktop-recovery.*.tmp' 'desktop-automation-cooldown.*.tmp' \
    '.auto-reset-state.*.tmp'
remove_matching_files "$RECOVERY_DIR" \
    'restart-*.log' 'restart-*.claimed' 'banner-*.ready'

if [ -d "$RECOVERY_DIR" ]; then
    /bin/rmdir "$RECOVERY_DIR" 2>/dev/null ||
        warn "preserving non-empty recovery directory: $RECOVERY_DIR"
fi

# Clear the preference domains as well as their plist files. This is scoped to
# the two bundle identifiers owned by this project and does not touch ChatGPT.
if command -v defaults >/dev/null 2>&1; then
    /usr/bin/defaults delete com.codex.monitor >/dev/null 2>&1 || true
    /usr/bin/defaults delete com.dst.codex-monitor >/dev/null 2>&1 || true
    /usr/bin/defaults delete "${NOTIFIER_BUNDLE_ID}" >/dev/null 2>&1 || true
fi
for path in "${ALWAYS_ARTIFACT_PATHS[@]}"; do remove_path "$path"; done

# Remove the marker last. It is app-owned, not a shared credential/transcript.
remove_path "${RECOVERY_DIR}/cancel-restart"

# The installer leaves this coordination file in TMPDIR. Remove it only after
# the app and workers have stopped and only when no other installer holds it.
remove_unlocked_file "$INSTALL_LOCK_FILE"

if [ "$FAILED" -ne 0 ]; then
    warn 'uninstall completed with warnings; inspect the paths reported above'
    exit 1
fi
note 'Codex Monitor & Switcher uninstall complete.'
