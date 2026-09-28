#!/bin/bash
set -euo pipefail

# ==============================================================================
# 🚀 OpenAI Codex Monitor & Switcher — Universal macOS Installer
# ==============================================================================

# Platform check — macOS only (Apple Silicon & Intel)
if [ "$(uname -s)" != "Darwin" ]; then
    echo "❌ Error: Codex Monitor is designed exclusively for macOS (Apple Silicon & Intel)."
    echo "   Linux and Windows are not currently supported."
    exit 1
fi

APP_NAME="Codex Monitor"
BUNDLE_NAME="${APP_NAME}.app"
REPO_URL="${REPO_URL:-https://github.com/dst0/openai-usage-monitor.git}"
LOCAL_BIN="${HOME}/.local/bin"
mkdir -p "${LOCAL_BIN}"

ensure_private_monitor_logs() {
    # All Monitor log creation and mode changes happen in the fd-anchored Rust
    # service. The shell only selects the already-built Monitor executable;
    # launchd is configured later, after this command succeeds.
    local codex_home="${CODEX_HOME:-${HOME}/.codex}"
    case "${codex_home}" in
        /*) ;;
        *) echo "❌ Refusing installation: Monitor home must be an absolute path."; return 1 ;;
    esac
    CODEX_HOME="${codex_home}" "${LOCAL_BIN}/codex-mon" monitor-logs --install >/dev/null || {
        echo "❌ Refusing installation: could not prepare private Monitor logs."
        return 1
    }
}

retire_launchd_job() {
    local label="$1"
    local plist="${2:-}"
    local expected_executable="${3:-}"
    local attempt=0
    local output="" pid="" executable="" state
    if output="$(/bin/launchctl list "${label}" 2>/dev/null)"; then
        if [[ "${output}" == *'"PID"'* ]]; then
            pid="$(/usr/bin/printf '%s\n' "${output}" | /usr/bin/sed -n 's/.*"PID" = \([0-9][0-9]*\);.*/\1/p' | /usr/bin/head -n 1)"
            case "${pid}" in
                *[!0-9]*|'') echo "❌ Refusing log migration: invalid launchd PID for ${label}."; return 1 ;;
            esac
            if /bin/kill -0 "${pid}" 2>/dev/null; then
                executable="$(/bin/ps -p "${pid}" -o comm= 2>/dev/null | /usr/bin/sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
                case "${executable}" in
                    "${expected_executable}"|"${LOCAL_BIN}/cxi") ;;
                    *) echo "❌ Refusing log migration: launchd PID for ${label} has an unexpected executable."; return 1 ;;
                esac
            else
                pid=""
            fi
        fi
    else
        if launchd_job_present "${label}"; then
            echo "❌ Refusing log migration: launchd job ${label} cannot be inspected."
            return 1
        else
            state=$?
        fi
        if [ "${state}" -ne 1 ]; then
            echo "❌ Refusing log migration: launchd state cannot be verified."
            return 1
        fi
    fi
    if [ -n "${plist}" ] && [ -f "${plist}" ]; then
        /bin/launchctl unload "${plist}" 2>/dev/null || true
    fi
    /bin/launchctl remove "${label}" 2>/dev/null || true
    while true; do
        if launchd_job_present "${label}"; then
            attempt=$((attempt + 1))
            if [ "${attempt}" -ge 50 ]; then
                echo "❌ Refusing log migration: launchd job ${label} is still active."
                return 1
            fi
            sleep 0.1
            continue
        else
            state=$?
        fi
        if [ "${state}" -eq 1 ]; then
            break
        fi
        echo "❌ Refusing log migration: launchd state cannot be verified."
        return 1
    done
    attempt=0
    while [ -n "${pid}" ] && /bin/kill -0 "${pid}" 2>/dev/null; do
        executable="$(/bin/ps -p "${pid}" -o comm= 2>/dev/null | /usr/bin/sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
        [ "${executable}" = "${expected_executable}" ] || [ "${executable}" = "${LOCAL_BIN}/cxi" ] || break
        attempt=$((attempt + 1))
        if [ "${attempt}" -ge 50 ]; then
            echo "❌ Refusing log migration: launchd PID for ${label} is still active."
            return 1
        fi
        sleep 0.1
    done
}

wait_for_restart_worker() {
    "${PROJECT_DIR}/scripts/wait_for_restart_worker.sh"
}

monitor_process_ids() {
    local output status
    if output="$(/usr/bin/pgrep -x "CodexMonitor" 2>/dev/null)"; then
        /usr/bin/printf '%s\n' "${output}"
        return 0
    else
        status=$?
    fi
    if [ "${status}" -eq 1 ]; then
        return 0
    fi
    echo "❌ Refusing log migration: Monitor processes cannot be enumerated." >&2
    return 1
}

stop_monitor_log_writers() {
    local pid identity pids snapshots remaining attempt=0
    pids="$(monitor_process_ids)" || return 1
    snapshots=""

    # Validate every candidate before changing launchd or process state. This
    # keeps an unexpected same-name process as a completely read-only failure.
    for pid in ${pids}; do
        case "${pid}" in
            *[!0-9]*|'') echo "❌ Refusing log migration: invalid Monitor PID."; return 1 ;;
        esac
        identity="$(monitor_process_identity "${pid}")" || {
            echo "❌ Refusing log migration: CodexMonitor PID has an unexpected executable."
            return 1
        }
        snapshots="${snapshots}${pid}|${identity}"$'\n'
    done

    # From the first state-changing operation onward, the EXIT trap must
    # restore the available installed app if any later quiescence step fails.
    WRITERS_QUIESCED=1
    # Stop the producer before waiting for an already-submitted one-shot worker.
    retire_launchd_job \
        "com.codex.switcher" \
        "${HOME}/Library/LaunchAgents/com.codex.switcher.plist" \
        "${LOCAL_BIN}/codex-mon"
    while IFS='|' read -r pid expected_executable expected_started_at; do
        [ -n "${pid}" ] || continue
        terminate_verified_monitor_process \
            "${pid}" \
            "${expected_executable}|${expected_started_at}" || return 1
    done <<< "${snapshots}"

    while true; do
        remaining="$(monitor_process_ids)" || return 1
        [ -n "${remaining}" ] || break
        attempt=$((attempt + 1))
        if [ "${attempt}" -ge 50 ]; then
            echo "❌ Refusing log migration: Codex Monitor did not exit cleanly."
            return 1
        fi
        sleep 0.1
    done
    wait_for_restart_worker
}

# Detect if running from local repository or piped via curl
SCRIPT_SOURCE="${BASH_SOURCE[0]:-$0}"
PROJECT_DIR=""
if [ -f "${SCRIPT_SOURCE}" ]; then
    POTENTIAL_DIR="$(cd "$(dirname "${SCRIPT_SOURCE}")/.." && pwd)"
    if [ -f "${POTENTIAL_DIR}/codex-switcher/Cargo.toml" ] && [ -f "${POTENTIAL_DIR}/Sources/main.swift" ]; then
        PROJECT_DIR="${POTENTIAL_DIR}"
    fi
fi

CLEANUP_TMP=0
CLI_STAGING=""
PERSISTENT_SKILL_ROOT=""
WRITERS_QUIESCED=0
INSTALL_SUCCEEDED=0
APP_STAGE_ROOT=""
APP_STAGING_PATH=""
APP_BACKUP_ROOT=""
APP_BACKUP_PATH=""
APP_TARGET_PATH=""
APP_SWAP_ACTIVE=0
APP_HAD_EXISTING_TARGET=0
cleanup() {
    if [ -n "${CLI_STAGING}" ]; then
        # codesign --force writes <file>.cstemp next to the file it signs.
        rm -f "${CLI_STAGING}" "${CLI_STAGING}.cstemp"
    fi
    if [ "${CLEANUP_TMP}" -eq 1 ] && [ -d "${TMP_DIR:-}" ]; then
        rm -rf "${TMP_DIR}"
    fi
    if [ "${APP_SWAP_ACTIVE}" -eq 1 ] && [ "${INSTALL_SUCCEEDED}" -eq 0 ]; then
        if ! rollback_app_bundle_swap; then
            echo "⚠️  Could not roll back the prior Monitor app automatically."
            echo "   Preserved backup: ${APP_BACKUP_PATH}"
        fi
    fi
    cleanup_app_bundle_swap_paths 2>/dev/null || true
    if [ "${WRITERS_QUIESCED}" -eq 1 ] && [ "${INSTALL_SUCCEEDED}" -eq 0 ]; then
        if [ -x "${INSTALL_DIR}/${BUNDLE_NAME}/Contents/MacOS/CodexMonitor" ]; then
            echo "⚠️  Installation stopped after quiescing Monitor writers; restoring the available app."
            /usr/bin/open "${INSTALL_DIR}/${BUNDLE_NAME}" >/dev/null 2>&1 || true
        fi
    fi
    # The install lock outlives this cleanup: its keeper (or, without perl,
    # this shell's descriptor 9) is released only when the installer exits,
    # after the temporary paths above are gone. The uninstaller treats a free
    # lock as proof that no installer owns them.
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# bash runs no EXIT trap when SIGQUIT (Ctrl-\) ends it; exit through the trap.
trap 'exit 131' QUIT

# ------------------------------------------------------------------------------
# Concurrency Lock: Serialize installation runs across terminal sessions & projects
# ------------------------------------------------------------------------------
# Take the lock before creating any temporary path (remote clone, CLI and app
# staging, app backup); cleanup() releases it only after removing them. The
# uninstaller removes such leftovers only while no installer holds this lock.
# A one-line install has no helper scripts before its clone, so the lock is
# defined here. tests/install_lock.sh and tests/log_permissions_and_uninstall.sh
# source the lines between the two markers; keep them free of other commands.
# >>> install lock
INSTALL_UID="$(/usr/bin/id -u)"
INSTALL_LOCK_FILE=""
# "<owner uid> <device>:<inode> <type>" of the lock file this installer holds.
INSTALL_LOCK_IDENTITY=""
# Seconds between attempts while another installer holds the lock.
INSTALL_LOCK_POLL_SECONDS=1
# How often the lock file may turn out to have been replaced before giving up.
INSTALL_LOCK_MAX_OPENS=20

# The lock lives in the per-user temporary directory, which `mktemp -t` also
# uses, whatever TMPDIR says, so installers and uninstallers started from a
# shell, launchd, or the Menu Bar app agree on one file. Prints the directory
# without its trailing slash; fails unless it is an absolute path naming a
# directory, not a symlink, owned by this user.
install_lock_dir() {
    local dir
    dir="$(/usr/bin/getconf DARWIN_USER_TEMP_DIR 2>/dev/null)" || return 1
    dir="${dir%/}"
    case "${dir}" in
        /?*) ;;
        *) return 1 ;;
    esac
    [ ! -L "${dir}" ] && [ -d "${dir}" ] &&
        [ "$(/usr/bin/stat -f '%u' "${dir}" 2>/dev/null)" = "${INSTALL_UID}" ] || return 1
    /usr/bin/printf '%s\n' "${dir}"
}

# Tries once to take the exclusive flock(2) lock of the file open as
# descriptor $1. Exit status 0: taken (or already held through this
# descriptor); 75 (EX_TEMPFAIL): another open file holds it; anything else:
# unknown. /usr/bin/lockf, and its descriptor form, first ship with macOS 15;
# macOS 13 and 14 have perl's flock, the same lock. A lockf without the
# descriptor form rejects it as a usage error (64), which falls back too.
# Callers wait by retrying: lockf's own wait on a descriptor spins a CPU.
# perl runs with -T, which ignores PERL5OPT and PERL5LIB from the caller's
# environment. scripts/uninstall.sh keeps an identical copy of this function.
flock_fd_now() {
    local fd="$1"
    local status=69
    if [ -x /usr/bin/lockf ]; then
        /usr/bin/lockf -s -t 0 "${fd}" 2>/dev/null && return 0 || status=$?
        [ "${status}" -eq 64 ] || return "${status}"
    fi
    [ -x /usr/bin/perl ] || return 69
    /usr/bin/perl -T -MErrno -MFcntl=:flock -e '
        open(my $lock, "<&=", $ARGV[0]) or exit 71;
        flock($lock, LOCK_EX | LOCK_NB) or exit($!{EWOULDBLOCK} ? 75 : 71);
        exit 0;
    ' "${fd}" 2>/dev/null
}

# "<owner uid> <device>:<inode> <type>" of the file open as descriptor 9, or
# with an argument, of that path without following a symlink. `stat /dev/fd/9`
# would report the device of devfs instead of the file's.
install_lock_identity() {
    if [ "$#" -eq 0 ]; then
        /usr/bin/stat -f '%u %d:%i %HT' 0<&9 2>/dev/null
    else
        /usr/bin/stat -f '%u %d:%i %HT' "$1" 2>/dev/null
    fi
}

# Moves the lock off this shell: a keeper process inherits descriptor 9,
# ignores interrupts, and exits once this shell has exited (its parent
# changes); this shell then closes descriptor 9. No command the installer
# starts, such as a compiler cache server that outlives cargo, can then
# inherit the lock and keep it after the installer ends, and an interrupted
# installer keeps the lock until its EXIT cleanup is done. The keeper frees
# the lock within about 0.05 s of the installer's exit. It reports that it
# runs, as this shell's child with descriptor 9 open, before this shell
# closes descriptor 9; otherwise (or without perl) this shell keeps it.
# Never add a bare `wait` to the installer: it would wait for the keeper,
# which waits for the installer to exit.
hand_off_install_lock() {
    [ -x /usr/bin/perl ] || return 0
    local saved_traps
    local ready=""
    # The keeper inherits these as ignored, so no signal can end it before
    # perl starts; this shell then restores its own traps. A signal that
    # arrives in between is lost.
    saved_traps="$(trap -p INT TERM HUP QUIT)"
    trap '' INT TERM HUP QUIT
    if { exec 8< <(exec /usr/bin/perl -T -e '
            $SIG{$_} = "IGNORE" for qw(INT TERM HUP QUIT);
            my $installer = shift;
            open(my $lock, "<&=", 9) or exit 71;
            exit 72 unless getppid() == $installer;
            $| = 1;
            print "ready\n";
            close(STDOUT);
            select(undef, undef, undef, 0.05) while getppid() == $installer;
        ' "$$" 0</dev/null 2>/dev/null); } 2>/dev/null; then
        IFS= read -r -t 10 ready <&8 || ready=""
        exec 8<&-
    fi
    trap - INT TERM HUP QUIT
    eval "${saved_traps}"
    if [ "${ready}" = ready ]; then
        exec 9<&-
    else
        echo "⚠️  The install lock keeper did not start; commands this installer runs share its lock."
    fi
}

# Fails unless the lock path still names the file this installer locked and
# that lock is still held. Current uninstallers remove a lock file only while
# holding its lock, but those from before the per-user temporary directory
# lock removed it once more after releasing it, which could remove a newer
# installer's file and let another installer run alongside it; and a keeper
# that ended early (killed, for example) freed the lock. A probe through a
# new open file finds a held lock busy, whether the keeper or this shell
# holds it. Checked before each step that creates a path an uninstaller must
# not remove while an installer runs.
install_lock_still_named() {
    local status=0
    if [ -z "${INSTALL_LOCK_IDENTITY}" ] ||
        [ "$(install_lock_identity "${INSTALL_LOCK_FILE}")" != "${INSTALL_LOCK_IDENTITY}" ]; then
        echo "❌ Stopping installation: another program removed or replaced the install lock file: ${INSTALL_LOCK_FILE}"
        return 1
    fi
    if { exec 8<"${INSTALL_LOCK_FILE}"; } 2>/dev/null; then
        flock_fd_now 8 || status=$?
        exec 8<&-
        [ "${status}" -ne 75 ] || return 0
    fi
    echo "❌ Stopping installation: this installer no longer holds the install lock: ${INSTALL_LOCK_FILE}"
    return 1
}

# Opens INSTALL_LOCK_FILE as descriptor 9 and takes its lock, waiting while
# another installer holds it. The uninstaller removes a lock file while it
# holds the lock, so a waiter can acquire a file that no longer has a name,
# which nobody else can see or lock. Once locked, the path must therefore
# still name the locked file; otherwise the file now at the path is opened
# and locked instead. Fails closed when the lock cannot be taken. A new lock
# file is private to this user whatever the umask, and an existing one that
# is only readable is still locked (flock needs no write access).
acquire_install_lock() {
    local dir locked holder_pid saved_umask
    local opens=0
    local announced=0
    local status=0
    dir="$(install_lock_dir)" || {
        echo "❌ Refusing installation: the per-user temporary directory (DARWIN_USER_TEMP_DIR) is unavailable."
        return 1
    }
    INSTALL_LOCK_FILE="${dir}/codex_monitor_install_${INSTALL_UID}.lock"
    while true; do
        opens=$((opens + 1))
        if [ "${opens}" -gt "${INSTALL_LOCK_MAX_OPENS}" ]; then
            echo "❌ Refusing installation: the install lock file kept changing: ${INSTALL_LOCK_FILE}"
            return 1
        fi
        if [ -L "${INSTALL_LOCK_FILE}" ] || { [ -e "${INSTALL_LOCK_FILE}" ] && [ ! -f "${INSTALL_LOCK_FILE}" ]; }; then
            echo "❌ Refusing installation: the install lock is not a regular file: ${INSTALL_LOCK_FILE}"
            return 1
        fi
        saved_umask="$(umask)"
        umask 077
        if ! { exec 9<>"${INSTALL_LOCK_FILE}"; } 2>/dev/null && ! { exec 9<"${INSTALL_LOCK_FILE}"; } 2>/dev/null; then
            umask "${saved_umask}"
            echo "❌ Refusing installation: the install lock cannot be opened: ${INSTALL_LOCK_FILE}"
            return 1
        fi
        umask "${saved_umask}"
        while true; do
            if flock_fd_now 9; then
                break
            else
                status=$?
            fi
            if [ "${status}" -ne 75 ]; then
                exec 9<&-
                if [ "${status}" -eq 69 ]; then
                    echo "❌ Refusing installation: the install lock needs /usr/bin/lockf or /usr/bin/perl."
                else
                    echo "❌ Refusing installation: the install lock cannot be taken (status ${status}): ${INSTALL_LOCK_FILE}"
                fi
                return 1
            fi
            if [ "${announced}" -eq 0 ]; then
                holder_pid="$(/usr/bin/head -n 1 "${INSTALL_LOCK_FILE}" 2>/dev/null || true)"
                case "${holder_pid}" in
                    ''|*[!0-9]*) echo "⏳ Another installation is currently in progress. Waiting for it to finish..." ;;
                    *) echo "⏳ Another installation (PID ${holder_pid}) is currently in progress. Waiting for it to finish..." ;;
                esac
                announced=1
            fi
            /bin/sleep "${INSTALL_LOCK_POLL_SECONDS}"
        done
        locked="$(install_lock_identity)"
        if [ -n "${locked}" ] && [ "${locked}" = "$(install_lock_identity "${INSTALL_LOCK_FILE}")" ]; then
            case "${locked}" in
                "${INSTALL_UID} "*" Regular File") break ;;
            esac
            exec 9<&-
            echo "❌ Refusing installation: the install lock is not this user's regular file: ${INSTALL_LOCK_FILE}"
            return 1
        fi
        exec 9<&-
        echo "🔁 The install lock file was replaced while waiting; locking the current one..."
    done
    [ "${announced}" -eq 0 ] || echo "🔒 Acquired installation lock. Continuing..."
    INSTALL_LOCK_IDENTITY="${locked}"
    # Tell the next waiter who holds the lock. Written through the locked
    # descriptor: writing by path would create an unlocked file if the path
    # changed. A longer earlier line may remain after it; only the first line
    # is read.
    /usr/bin/printf '%s\n' "$$" >&9 2>/dev/null || true
    hand_off_install_lock
}
# <<< install lock

acquire_install_lock || exit 1

if [ -z "${PROJECT_DIR}" ]; then
    echo "🌐 Remote installation detected. Preparing temporary build environment..."
    install_lock_still_named || exit 1
    TMP_DIR="$(mktemp -d -t codex-mon-install-XXXXXX)"
    CLEANUP_TMP=1

    echo "📥 Fetching source code from ${REPO_URL}..."
    if command -v git >/dev/null 2>&1; then
        git clone --depth 1 "${REPO_URL}" "${TMP_DIR}"
    else
        echo "❌ Git is required to clone the repository."
        echo "👉 Install Apple Command Line Tools by running: xcode-select --install"
        exit 1
    fi
    PROJECT_DIR="${TMP_DIR}"
    # The temporary clone is removed on exit, so remote installs must retain a
    # small, app-owned copy of the skills before linking them into agent homes.
    PERSISTENT_SKILL_ROOT="${HOME}/.local/share/codex-monitor/skills"
fi

source "${PROJECT_DIR}/scripts/install_bundle_swap.sh"
source "${PROJECT_DIR}/scripts/install_launchd_helpers.sh"
source "${PROJECT_DIR}/scripts/install_monitor_process_guard.sh"
source "${PROJECT_DIR}/scripts/swift_module_cache.sh"

BUILD_DIR="${PROJECT_DIR}/build"
APP_DIR="${BUILD_DIR}/${BUNDLE_NAME}"

# Detect installation directory: prefer /Applications, fallback to ~/Applications
if [ -w "/Applications" ]; then
    INSTALL_DIR="/Applications"
else
    INSTALL_DIR="${HOME}/Applications"
fi
mkdir -p "${INSTALL_DIR}"

# ------------------------------------------------------------------------------
# Check Prerequisites: Swift & Rust
# ------------------------------------------------------------------------------
if ! command -v swiftc >/dev/null 2>&1; then
    echo "❌ Swift compiler (swiftc) not found."
    echo "👉 Install Apple Command Line Tools by running:"
    echo "   xcode-select --install"
    exit 1
fi
# Every swiftc call below must use the scripts' own module cache subdirectory:
# a cache reused through another spelling, such as /tmp for /private/tmp, fails
# to compile. See scripts/swift_module_cache.sh.
canonicalize_clang_module_cache_path || exit 1

if ! command -v cargo >/dev/null 2>&1; then
    if [ -f "${HOME}/.cargo/env" ]; then
        source "${HOME}/.cargo/env"
    fi
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "❌ Rust / Cargo not found."
    echo "👉 Install Rust in one command by running:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "After installation completes, re-run this script."
    exit 1
fi

# Desktop recovery is an optional integration. Keep CLI-only installation
# usable, but make the standard Desktop prerequisite visible before building.
CODEX_DESKTOP_APP="/Applications/ChatGPT.app"
if { [ ! -x "${CODEX_DESKTOP_APP}/Contents/Resources/codex-cli/bin/codex" ] ||
     [ ! -x "${CODEX_DESKTOP_APP}/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex" ]; } &&
   [ ! -x "${CODEX_DESKTOP_APP}/Contents/Resources/codex" ]; then
    echo "⚠️  Official Codex Desktop was not found at ${CODEX_DESKTOP_APP}."
    echo "   CLI quota monitoring will work; Desktop restart/recovery needs the normal OpenAI app installation."
fi

# Ensure ~/.local/bin is in PATH
if [[ ":$PATH:" != *":${LOCAL_BIN}:"* ]]; then
    export PATH="${LOCAL_BIN}:${PATH}"
    for rc in "${HOME}/.zshrc" "${HOME}/.bash_profile"; do
        if [ -f "${rc}" ] || [ "${rc}" = "${HOME}/.zshrc" ]; then
            if ! grep -q '\.local/bin' "${rc}" 2>/dev/null; then
                echo '' >> "${rc}"
                echo '# OpenAI Codex Monitor CLI path' >> "${rc}"
                echo 'export PATH="$HOME/.local/bin:$PATH"' >> "${rc}"
                echo "✨ Added ~/.local/bin to PATH in ${rc}"
            fi
        fi
    done
fi

ARCH="$(uname -m)"
echo "🖥️  Detected macOS (${ARCH}). Starting compilation..."

# ------------------------------------------------------------------------------
# 1. Build Rust CLI (codex-mon / cxi)
# ------------------------------------------------------------------------------
echo "🦀 [1/4] Building Rust CLI (codex-mon / cxi)..."
cd "${PROJECT_DIR}/codex-switcher"
# rust-toolchain.toml pins the exact compiler. Install it up front (rustup >=
# 1.28) so the build does not depend on rustup's auto-install setting; older
# rustup releases lack the no-argument form but auto-install on first use.
if ! command -v rustup >/dev/null 2>&1; then
    echo "⚠️  rustup not found; building with $(rustc --version 2>/dev/null || echo 'the installed rustc') instead of the pinned toolchain."
elif ! rustup toolchain install; then
    echo "⚠️  Could not pre-install the pinned Rust toolchain; relying on rustup auto-install."
fi
# Build exactly the committed Cargo.lock. --locked fails closed when the
# lockfile is missing or out of date instead of shipping newly resolved crates.
if ! cargo build --release --locked; then
    echo "❌ Rust build failed."
    echo "   If the error says the lock file cannot be created or updated because of --locked,"
    echo "   this checkout's codex-switcher/Cargo.lock is missing or does not match Cargo.toml."
    echo "   Use an unmodified checkout; do not drop --locked, which would build unreviewed"
    echo "   dependency versions."
    echo "   Cargo older than 1.78 cannot read this lockfile format (version 4): install"
    echo "   Rust with rustup so the toolchain pinned in rust-toolchain.toml is used."
    exit 1
fi

echo "📦 Installing CLI to ${LOCAL_BIN}..."
# The daemon may be executing the current CLI binary. Rewriting that inode in
# place invalidates its mapped code signature and can make subsequent settings
# commands die with SIGKILL. Prepare, freshly sign, and verify a fresh inode,
# then atomically replace the pathname so the old daemon can finish safely.
install_lock_still_named || exit 1
CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"
cp "target/release/codex-mon" "${CLI_STAGING}"
chmod 755 "${CLI_STAGING}"
xattr -c "${CLI_STAGING}" 2>/dev/null || true
codesign --sign - --force "${CLI_STAGING}"
codesign --verify --strict "${CLI_STAGING}"
"${CLI_STAGING}" --version >/dev/null
mv -f "${CLI_STAGING}" "${LOCAL_BIN}/codex-mon"
CLI_STAGING=""
ln -sfn "${LOCAL_BIN}/codex-mon" "${LOCAL_BIN}/cxi"

# Ensure transparent codex CLI shim exists
echo "🔗 Configuring codex CLI shim..."
if ! "${LOCAL_BIN}/codex-mon" install-shim; then
    echo "⚠️  Could not install codex CLI shim; any existing codex command was preserved."
fi

# Compile the native recovery banner and visibility helper
if [ -f "${PROJECT_DIR}/scripts/codex-ui-resume.swift" ]; then
    echo "⚡ Compiling codex-ui-resume helper..."
    swiftc -O -target "${ARCH}-apple-macosx13.0" -o "${LOCAL_BIN}/codex-ui-resume" "${PROJECT_DIR}/scripts/codex-ui-resume.swift"
    chmod +x "${LOCAL_BIN}/codex-ui-resume"
fi

# Recovery presentation and window restoration are separate PID-bound helpers.
# They receive only the private payload path or exact process identity; no raw
# session metadata is passed through argv.
if [ -f "${PROJECT_DIR}/Sources/CodexRecoveryBanner.swift" ] && [ -f "${PROJECT_DIR}/scripts/codex-recovery-banner-main.swift" ]; then
    echo "⚡ Compiling Codex recovery banner helper..."
    RECOVERY_BANNER_SOURCES=()
    while IFS= read -r recovery_source || [ -n "${recovery_source}" ]; do
        [ -n "${recovery_source}" ] || continue
        RECOVERY_BANNER_SOURCES+=("${PROJECT_DIR}/${recovery_source}")
    done < "${PROJECT_DIR}/scripts/codex-recovery-banner-sources.txt"
    swiftc -O -target "${ARCH}-apple-macosx13.0" \
        -framework AppKit -framework Foundation -framework ApplicationServices \
        -o "${LOCAL_BIN}/codex-recovery-banner" \
        "${RECOVERY_BANNER_SOURCES[@]}" \
        "${PROJECT_DIR}/scripts/codex-recovery-banner-main.swift"
    chmod +x "${LOCAL_BIN}/codex-recovery-banner"
fi
if [ -f "${PROJECT_DIR}/scripts/codex-window-restore.swift" ]; then
    echo "⚡ Compiling Codex window restore helper..."
    swiftc -O -target "${ARCH}-apple-macosx13.0" \
        -framework AppKit -framework Foundation -framework ApplicationServices \
        -o "${LOCAL_BIN}/codex-window-restore" \
        "${PROJECT_DIR}/scripts/CodexWindowAXValueDecoder.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowSafetyChecks.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskProbeValidation.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskProbeKeyboard.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskProbeCore.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskSessionSystem.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskSessionCore.swift" \
        "${PROJECT_DIR}/scripts/CodexPreservedClipboard.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskRecords.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskProbe.swift" \
        "${PROJECT_DIR}/scripts/CodexWindowTaskSession.swift" \
        "${PROJECT_DIR}/scripts/codex-window-restore.swift"
    chmod +x "${LOCAL_BIN}/codex-window-restore"
fi

# Compile and install native Codex Notifier helper
if [ -f "${PROJECT_DIR}/codex-notifier/build.sh" ]; then
    echo "🔔 Building and installing Codex Notifier..."
    "${PROJECT_DIR}/codex-notifier/build.sh"
fi

# ------------------------------------------------------------------------------
# 2. Build macOS Menu Bar App (Codex Monitor.app)
# ------------------------------------------------------------------------------
echo ""
echo "🔨 [2/4] Building native macOS Menu Bar App (${APP_NAME})..."
cd "${PROJECT_DIR}"
rm -rf "${BUILD_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

# Copy Info.plist
cp "${PROJECT_DIR}/resources/Info.plist" "${APP_DIR}/Contents/Info.plist"

# Copy AppIcon
if [ -f "${PROJECT_DIR}/resources/AppIcon.icns" ]; then
    cp "${PROJECT_DIR}/resources/AppIcon.icns" "${APP_DIR}/Contents/Resources/AppIcon.icns"
fi

# Copy Status Bar Icon
if [ -f "${PROJECT_DIR}/resources/statusbar_icon.png" ]; then
    cp "${PROJECT_DIR}/resources/statusbar_icon"*.png "${APP_DIR}/Contents/Resources/" 2>/dev/null || true
fi

# Copy Help Documentation
if [ -f "${PROJECT_DIR}/resources/helps.html" ]; then
    cp "${PROJECT_DIR}/resources/helps.html" "${APP_DIR}/Contents/Resources/helps.html"
    mkdir -p "${HOME}/.codex"
    cp "${PROJECT_DIR}/resources/helps.html" "${HOME}/.codex/helps.html"
fi

# Bundle the confirmation-gated uninstaller so the running Menu Bar app can
# remove itself even when the installation came from a temporary remote clone.
if [ -f "${PROJECT_DIR}/scripts/uninstall.sh" ]; then
    cp "${PROJECT_DIR}/scripts/uninstall.sh" "${APP_DIR}/Contents/Resources/uninstall.sh"
    chmod 755 "${APP_DIR}/Contents/Resources/uninstall.sh"
fi

# Copy localization bundles (*.lproj)
for lproj in "${PROJECT_DIR}/resources/"*.lproj; do
    if [ -d "$lproj" ]; then
        cp -R "$lproj" "${APP_DIR}/Contents/Resources/"
    fi
done

SWIFT_SOURCES=(
    "${PROJECT_DIR}/Sources/Localization.swift"
    "${PROJECT_DIR}/Sources/QuotaModels.swift"
    "${PROJECT_DIR}/Sources/StatusBarStyle.swift"
    "${PROJECT_DIR}/Sources/CodexClient.swift"
    "${PROJECT_DIR}/Sources/CodexDesktopProcessIdentity.swift"
    "${PROJECT_DIR}/Sources/CodexRecoveryProcessIdentity.swift"
    "${PROJECT_DIR}/Sources/AutoLaunchManager.swift"
    "${PROJECT_DIR}/Sources/LaunchAtLoginMenuController.swift"
    "${PROJECT_DIR}/Sources/SingleInstanceGuard.swift"
    "${PROJECT_DIR}/Sources/MenuIconButton.swift"
    "${PROJECT_DIR}/Sources/InsetSeparatorView.swift"
    "${PROJECT_DIR}/Sources/PrimaryMenuSectionHeaderView.swift"
    "${PROJECT_DIR}/Sources/AccountSectionHeaderView.swift"
    "${PROJECT_DIR}/Sources/ReserveAccountSectionEntry.swift"
    "${PROJECT_DIR}/Sources/ResetCreditsRowView.swift"
    "${PROJECT_DIR}/Sources/AccountRowView.swift"
    "${PROJECT_DIR}/Sources/AccountSwitchButtonsView.swift"
    "${PROJECT_DIR}/Sources/AccountSectionCardView.swift"
    "${PROJECT_DIR}/Sources/AccountSectionCardView+Tracking.swift"
    "${PROJECT_DIR}/Sources/AppDelegate.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+FileWatchers.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+DesktopLifecycle.swift"
    "${PROJECT_DIR}/Sources/StatusBarBracketRenderer.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+StatusBar.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+StatusBarOverloads.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+Menu.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+AppBlock.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+CliBlock.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+ReserveCards.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+DynamicItems.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+AccountActions.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+AutoSwitch.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+AutoReset.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+SettingsActions.swift"
    "${PROJECT_DIR}/Sources/AppDelegate+WindowBounds.swift"
    "${PROJECT_DIR}/Sources/main.swift"
)

echo "📦 Compiling Swift sources..."
swiftc \
    -O \
    -whole-module-optimization \
    -sdk "$(xcrun --show-sdk-path)" \
    -target "${ARCH}-apple-macosx13.0" \
    -framework AppKit \
    -framework Foundation \
    -framework ServiceManagement \
    -framework Security \
    -o "${APP_DIR}/Contents/MacOS/CodexMonitor" \
    "${SWIFT_SOURCES[@]}" \
    2>&1

echo "🔏 Ad-hoc code signing..."
codesign --force --deep --sign - "${APP_DIR}"
codesign --verify --deep --strict "${APP_DIR}"

# ------------------------------------------------------------------------------
# 3. Install to Applications & Register Login Item
# ------------------------------------------------------------------------------
echo ""
echo "📂 [3/4] Installing to ${INSTALL_DIR}..."

# Finish every fallible bundle copy/signature check while the installed app is
# still untouched and its writers remain available.
install_lock_still_named || exit 1
prepare_app_bundle_staging "${APP_DIR}" "${INSTALL_DIR}" "${BUNDLE_NAME}"
codesign --verify --deep --strict "${APP_STAGING_PATH}"

# Historical redaction atomically replaces changed Monitor-owned log inodes.
# Quiesce and verify every known writer only after all build/signing steps have
# succeeded, then migrate before replacing or relaunching the application.
stop_monitor_log_writers
ensure_private_monitor_logs
install_lock_still_named || exit 1
activate_app_bundle_staging "${INSTALL_DIR}/${BUNDLE_NAME}"

# If installing into /Applications, ensure ~/Applications has symlink
if [ "${INSTALL_DIR}" = "/Applications" ]; then
    if [ -d "${HOME}/Applications/${BUNDLE_NAME}" ] && [ ! -L "${HOME}/Applications/${BUNDLE_NAME}" ]; then
        rm -rf "${HOME}/Applications/${BUNDLE_NAME}"
    fi
    mkdir -p "${HOME}/Applications"
    rm -f "${HOME}/Applications/${BUNDLE_NAME}"
    ln -sfn "/Applications/${BUNDLE_NAME}" "${HOME}/Applications/${BUNDLE_NAME}"
fi

# Register in macOS Login Items
echo "⚙️  Configuring auto-start at Mac login..."
osascript -e "tell application \"System Events\"
    if exists (every login item whose name is \"${APP_NAME}\") then
        delete (every login item whose name is \"${APP_NAME}\")
    end if
    make login item at end with properties {name:\"${APP_NAME}\", path:\"${INSTALL_DIR}/${BUNDLE_NAME}\", hidden:false}
end tell" 2>/dev/null || true

# Configure the launchd daemon. Codex Monitor owns its lifecycle: opening the
# menu app loads it, and Quit unloads it together with any restart worker.
if [ -f "${PROJECT_DIR}/com.codex.switcher.plist" ]; then
    echo "⚙️  Configuring background monitoring daemon (launchd)..."
    LAUNCH_AGENTS="${HOME}/Library/LaunchAgents"
    mkdir -p "${LAUNCH_AGENTS}"
    TARGET_PLIST="${LAUNCH_AGENTS}/com.codex.switcher.plist"
    /bin/launchctl unload "${TARGET_PLIST}" 2>/dev/null || true
    sed "s|__HOME__|${HOME}|g" "${PROJECT_DIR}/com.codex.switcher.plist" > "${TARGET_PLIST}"
    echo "  -> Installed background daemon at ${TARGET_PLIST}; Codex Monitor will load it"
fi

# ------------------------------------------------------------------------------
# 4. Install Skills for AI Agents (Codex, Claude Code, Agent Swarms)
# ------------------------------------------------------------------------------
echo ""
echo "🧠 [4/4] Installing skills for Codex, Claude Code, and Agent Swarms..."
for skill_name in "cxi" "codex-mon"; do
    canonical_skill="${PROJECT_DIR}/skills/${skill_name}"
    if [ -d "${canonical_skill}" ]; then
        skill_target="${canonical_skill}"
        if [ -n "${PERSISTENT_SKILL_ROOT}" ]; then
            mkdir -p "${PERSISTENT_SKILL_ROOT}"
            rm -rf "${PERSISTENT_SKILL_ROOT}/${skill_name}"
            cp -R "${canonical_skill}" "${PERSISTENT_SKILL_ROOT}/${skill_name}"
            skill_target="${PERSISTENT_SKILL_ROOT}/${skill_name}"
            echo "  -> Retained remote skill source at ${skill_target}"
        fi
        for skill_dir in "${HOME}/.claude/skills" "${HOME}/.codex/skills" "${HOME}/.agents/skills"; do
            mkdir -p "${skill_dir}"
            rm -rf "${skill_dir}/${skill_name}"
            ln -sfn "${skill_target}" "${skill_dir}/${skill_name}"
            echo "  -> Linked ${skill_dir}/${skill_name}"
        done

        # Register skill in ~/.codex/config.toml if present and not already configured
        CODEX_CONFIG="${HOME}/.codex/config.toml"
        if [ -f "${CODEX_CONFIG}" ]; then
            if ! grep -q "/.codex/skills/${skill_name}/SKILL.md" "${CODEX_CONFIG}"; then
                echo "" >> "${CODEX_CONFIG}"
                echo "[[skills.config]]" >> "${CODEX_CONFIG}"
                echo "path = \"${HOME}/.codex/skills/${skill_name}/SKILL.md\"" >> "${CODEX_CONFIG}"
                echo "enabled = true" >> "${CODEX_CONFIG}"
                echo "  -> Registered ${skill_name} in ${CODEX_CONFIG}"
            fi
        fi
    fi
done

# ------------------------------------------------------------------------------
# Launch & Wrap Up
# ------------------------------------------------------------------------------
echo ""
commit_app_bundle_swap
cleanup_app_bundle_swap_paths
echo "🚀 Launching ${APP_NAME}..."
open "${INSTALL_DIR}/${BUNDLE_NAME}"
INSTALL_SUCCEEDED=1
WRITERS_QUIESCED=0

echo ""
echo "🎉 Installation complete!"
echo "   CLI: ${LOCAL_BIN}/cxi  (also: codex-mon, codex)"
echo "   App: ${INSTALL_DIR}/${BUNDLE_NAME}"
echo "   Look for the Codex icon (>_) in your macOS menu bar!"
echo ""
