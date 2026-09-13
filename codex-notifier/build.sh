#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="Codex Notifier.app"
BUILD_DIR="${SCRIPT_DIR}/build"
APP_BUNDLE="${BUILD_DIR}/${APP_NAME}"
INSTALL_DIR="${HOME}/Applications"

echo "==> [Codex Notifier] Preparing bundle structure..."
rm -rf "${APP_BUNDLE}"
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"

cp "${SCRIPT_DIR}/Info.plist" "${APP_BUNDLE}/Contents/Info.plist"
cp "${SCRIPT_DIR}/resources/AppIcon.icns" "${APP_BUNDLE}/Contents/Resources/AppIcon.icns"

echo "==> [Codex Notifier] Compiling native runner..."
clang -Wno-deprecated-declarations -O2 \
    -framework Cocoa \
    "${SCRIPT_DIR}/src/notify.m" \
    -o "${APP_BUNDLE}/Contents/MacOS/notify"

echo "==> [Codex Notifier] Signing app bundle..."
codesign -fs - "${APP_BUNDLE}"

echo "==> [Codex Notifier] Installing to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"
rm -rf "${INSTALL_DIR}/${APP_NAME}"
cp -R "${APP_BUNDLE}" "${INSTALL_DIR}/${APP_NAME}"

echo "==> [Codex Notifier] Registering with LaunchServices..."
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "${INSTALL_DIR}/${APP_NAME}"

echo "🎉 [Codex Notifier] Successfully built and installed: ${INSTALL_DIR}/${APP_NAME}"
