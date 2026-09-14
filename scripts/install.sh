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
cleanup() {
    if [ -n "${CLI_STAGING}" ] && [ -f "${CLI_STAGING}" ]; then
        rm -f "${CLI_STAGING}"
    fi
    if [ "${CLEANUP_TMP}" -eq 1 ] && [ -d "${TMP_DIR:-}" ]; then
        rm -rf "${TMP_DIR}"
    fi
}
trap cleanup EXIT INT TERM

if [ -z "${PROJECT_DIR}" ]; then
    echo "🌐 Remote installation detected. Preparing temporary build environment..."
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
# Concurrency Lock: Serialize installation runs across terminal sessions & projects
# ------------------------------------------------------------------------------
INSTALL_LOCK_FILE="${TMPDIR:-/tmp}/codex_monitor_install_${UID:-$(id -u)}.lock"
touch "${INSTALL_LOCK_FILE}"
exec 9>>"${INSTALL_LOCK_FILE}"

acquire_install_lock() {
    if command -v lockf >/dev/null 2>&1; then
        if ! lockf -s -t 0 9 2>/dev/null; then
            local holder_pid
            holder_pid="$(head -n 1 "${INSTALL_LOCK_FILE}" 2>/dev/null || true)"
            if [ -n "${holder_pid}" ] && kill -0 "${holder_pid}" 2>/dev/null; then
                echo "⏳ Another installation (PID ${holder_pid}) is currently in progress. Waiting for it to finish..."
            else
                echo "⏳ Another installation is currently in progress. Waiting for it to finish..."
            fi
            lockf 9
            echo "🔒 Acquired installation lock. Continuing..."
        fi
    elif command -v python3 >/dev/null 2>&1; then
        if ! python3 -c "import fcntl; fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)" 2>/dev/null; then
            local holder_pid
            holder_pid="$(head -n 1 "${INSTALL_LOCK_FILE}" 2>/dev/null || true)"
            if [ -n "${holder_pid}" ] && kill -0 "${holder_pid}" 2>/dev/null; then
                echo "⏳ Another installation (PID ${holder_pid}) is currently in progress. Waiting for it to finish..."
            else
                echo "⏳ Another installation is currently in progress. Waiting for it to finish..."
            fi
            python3 -c "import fcntl; fcntl.flock(9, fcntl.LOCK_EX)"
            echo "🔒 Acquired installation lock. Continuing..."
        fi
    fi
    echo "$$" > "${INSTALL_LOCK_FILE}"
}

acquire_install_lock

# ------------------------------------------------------------------------------
# Check Prerequisites: Swift & Rust
# ------------------------------------------------------------------------------
if ! command -v swiftc >/dev/null 2>&1; then
    echo "❌ Swift compiler (swiftc) not found."
    echo "👉 Install Apple Command Line Tools by running:"
    echo "   xcode-select --install"
    exit 1
fi

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
if [ ! -x "${CODEX_DESKTOP_APP}/Contents/Resources/codex" ]; then
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
cargo build --release

echo "📦 Installing CLI to ${LOCAL_BIN}..."
# The daemon may be executing the current CLI binary. Rewriting that inode in
# place invalidates its mapped code signature and can make subsequent settings
# commands die with SIGKILL. Prepare and verify a fresh inode, then atomically
# replace the pathname so the old daemon can finish safely.
CLI_STAGING="$(mktemp "${LOCAL_BIN}/.codex-mon.install.XXXXXX")"
cp "target/release/codex-mon" "${CLI_STAGING}"
chmod 755 "${CLI_STAGING}"
xattr -c "${CLI_STAGING}" 2>/dev/null || true
if ! codesign --verify --strict "${CLI_STAGING}" 2>/dev/null; then
    codesign --sign - --force "${CLI_STAGING}"
fi
codesign --verify --strict "${CLI_STAGING}"
"${CLI_STAGING}" --version >/dev/null
mv -f "${CLI_STAGING}" "${LOCAL_BIN}/codex-mon"
CLI_STAGING=""
ln -sfn "${LOCAL_BIN}/codex-mon" "${LOCAL_BIN}/cxi"

# Ensure transparent codex CLI shim exists
echo "🔗 Configuring codex CLI shim..."
"${LOCAL_BIN}/codex-mon" install-shim || true

# Compile the native recovery banner and visibility helper
if [ -f "${PROJECT_DIR}/scripts/codex-ui-resume.swift" ]; then
    echo "⚡ Compiling codex-ui-resume helper..."
    swiftc -O -target "${ARCH}-apple-macosx13.0" -o "${LOCAL_BIN}/codex-ui-resume" "${PROJECT_DIR}/scripts/codex-ui-resume.swift"
    chmod +x "${LOCAL_BIN}/codex-ui-resume"
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
    "${PROJECT_DIR}/Sources/AutoLaunchManager.swift"
    "${PROJECT_DIR}/Sources/SingleInstanceGuard.swift"
    "${PROJECT_DIR}/Sources/AppDelegate.swift"
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
codesign --force --deep --sign - "${APP_DIR}" 2>/dev/null || true

# ------------------------------------------------------------------------------
# 3. Install to Applications & Register Login Item
# ------------------------------------------------------------------------------
echo ""
echo "📂 [3/4] Installing to ${INSTALL_DIR}..."
# Stop previous running instances
pkill -x "CodexMonitor" 2>/dev/null || true
pkill -f "/Applications/${BUNDLE_NAME}" 2>/dev/null || true
pkill -f "${HOME}/Applications/${BUNDLE_NAME}" 2>/dev/null || true
sleep 0.5

rm -rf "${INSTALL_DIR}/${BUNDLE_NAME}"
cp -R "${APP_DIR}" "${INSTALL_DIR}/${BUNDLE_NAME}"
chmod -R 755 "${INSTALL_DIR}/${BUNDLE_NAME}"

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
    launchctl unload "${TARGET_PLIST}" 2>/dev/null || true
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
echo "🚀 Launching ${APP_NAME}..."
open "${INSTALL_DIR}/${BUNDLE_NAME}"

# Release concurrency lock explicitly
exec 9>&- 2>/dev/null || true

echo ""
echo "🎉 Installation complete!"
echo "   CLI: ${LOCAL_BIN}/cxi  (also: codex-mon, codex)"
echo "   App: ${INSTALL_DIR}/${BUNDLE_NAME}"
echo "   Look for the Codex icon (>_) in your macOS menu bar!"
echo ""
