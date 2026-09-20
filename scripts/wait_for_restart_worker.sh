#!/bin/bash
set -euo pipefail

LABEL="com.codex.switcher.restart-worker"
attempt=0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && /bin/pwd -P)"
source "${SCRIPT_DIR}/install_launchd_helpers.sh"

# A one-shot worker owns an in-flight Codex relaunch/recovery and removes this
# label itself. Never unload or remove it from the installer.
while true; do
    if launchd_job_present "${LABEL}"; then
        attempt=$((attempt + 1))
        if [ "${attempt}" -ge 6000 ]; then
            echo "❌ Refusing log migration: restart worker did not finish within 10 minutes."
            exit 1
        fi
        /bin/sleep 0.1
        continue
    else
        state=$?
    fi
    if [ "${state}" -eq 1 ]; then
        exit 0
    fi
    echo "❌ Refusing log migration: restart worker state could not be verified."
    exit 1
done
