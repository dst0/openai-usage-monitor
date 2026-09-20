#!/bin/bash

# Returns 0 when present, 1 only when a successful full listing proves absence,
# and 2 when launchctl state cannot be read reliably.
launchd_job_present() {
    local label="$1"
    local listing
    if /bin/launchctl list "${label}" >/dev/null 2>&1; then
        return 0
    fi
    if ! listing="$(/bin/launchctl list 2>/dev/null)"; then
        return 2
    fi
    if /usr/bin/printf '%s\n' "${listing}" |
        /usr/bin/awk -v expected="${label}" 'NF >= 3 && $NF == expected { found = 1 } END { exit found ? 0 : 1 }'; then
        return 0
    fi
    return 1
}
