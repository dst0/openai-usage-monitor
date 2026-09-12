#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_NAME="Codex Monitor"
BUNDLE_NAME="${APP_NAME}.app"
BUILD_DIR="${PROJECT_DIR}/build"
APP_DIR="${BUILD_DIR}/${BUNDLE_NAME}"

if [ -w "/Applications" ]; then
    INSTALL_DIR="/Applications"
else
    INSTALL_DIR="${HOME}/Applications"
fi

LOCAL_BIN="${HOME}/.local/bin"
mkdir -p "${LOCAL_BIN}"

echo "🦀 [1/4] Building Rust CLI (codex-mon / cxi)..."
cd "${PROJECT_DIR}/codex-switcher"
cargo build --release

echo "📦 Installing CLI to ${LOCAL_BIN}..."
cp "target/release/codex-mon" "${LOCAL_BIN}/codex-mon"
chmod +x "${LOCAL_BIN}/codex-mon"
ln -sfn "${LOCAL_BIN}/codex-mon" "${LOCAL_BIN}/cxi"

# Ensure codex CLI shim exists
echo "🔗 Configuring codex CLI shim..."
"${LOCAL_BIN}/codex-mon" install-shim || true

# Compile codex-ui-resume helper for Accessibility automation
echo "⚡ Compiling codex-ui-resume helper..."
swiftc -O -o "${LOCAL_BIN}/codex-ui-resume" "${PROJECT_DIR}/scripts/codex-ui-resume.swift"
chmod +x "${LOCAL_BIN}/codex-ui-resume"


echo ""
echo "🔨 [2/4] Building macOS Menu Bar App (${APP_NAME})..."
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
    -target arm64-apple-macosx13.0 \
    -framework AppKit \
    -framework Foundation \
    -framework ServiceManagement \
    -framework Security \
    -o "${APP_DIR}/Contents/MacOS/CodexMonitor" \
    "${SWIFT_SOURCES[@]}" \
    2>&1

echo "🔏 Ad-hoc code signing..."
codesign --force --deep --sign - "${APP_DIR}" 2>/dev/null || true

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

echo ""
echo "🧠 [4/4] Installing skills for Codex, Claude Code, and Agent Swarm..."
for skill_name in "cxi" "codex-mon"; do
    canonical_skill="${PROJECT_DIR}/skills/${skill_name}"
    if [ -d "${canonical_skill}" ]; then
        for skill_dir in "${HOME}/.claude/skills" "${HOME}/.codex/skills" "${HOME}/.agents/skills"; do
            mkdir -p "${skill_dir}"
            rm -rf "${skill_dir}/${skill_name}"
            ln -sfn "${canonical_skill}" "${skill_dir}/${skill_name}"
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

echo ""
echo "🚀 Launching ${APP_NAME}..."
open "${INSTALL_DIR}/${BUNDLE_NAME}"

echo ""
echo "🎉 Installation complete!"
echo "   CLI: ${LOCAL_BIN}/cxi  (also: codex-mon, codex)"
echo "   App: ${INSTALL_DIR}/${BUNDLE_NAME}"
echo "   Look for the blue Codex icon (>_) in your macOS menu bar!"
echo ""
