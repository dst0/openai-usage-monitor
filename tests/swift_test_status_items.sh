#!/bin/bash
# Static check: Swift tests remove every status item they create.
#
# A status item made with statusItem(withLength:) sits in the developer's real
# menu bar until removeStatusItem(_:) takes it out or the test binary exits, and
# nothing can list status items, so no runtime check can find one a test left
# behind. Every tests/*.swift file, at any depth, must therefore remove as many
# status items as it creates. Every call counts, several on one line too. Any
# receiver counts, so an alias of NSStatusBar.system is covered, and so does a
# call whose line ends at `statusItem(`, its argument on the next line. Lines
# whose first non-blank characters are // are ignored. The check counts calls
# and cannot pair them: review still checks that each removal follows its
# item's last use.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-status-item-lint.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT

CREATE='statusItem[[:space:]]*\([[:space:]]*(withLength|$)'
REMOVE='removeStatusItem[[:space:]]*\('

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# count FILE ERE -> how many times ERE matches in FILE outside // comment lines. Every
# match counts, so two calls on one line count twice.
count() {
    { /usr/bin/grep -E -v '^[[:space:]]*//' "$1" || true; } \
        | { /usr/bin/grep -o -E -- "$2" || true; } | /usr/bin/wc -l | /usr/bin/tr -d '[:space:]'
}

# check_tree ROOT -> prints each violation; returns 1 if there is any. A listing
# error fails the check rather than skipping part of the tree.
check_tree() {
    local root="$1" violations="" files file rel created removed status=0
    [ -d "${root}/tests" ] || { printf 'tests: no such directory under %s\n' "${root}"; return 1; }
    files="$(/usr/bin/find "${root}/tests" -type f -name '*.swift')" || status=$?
    if [ "${status}" -ne 0 ]; then
        printf 'tests: cannot list every Swift test (find exited %s)\n' "${status}"
        return 1
    fi
    while IFS= read -r file; do
        [ -n "${file}" ] || continue
        rel="${file#"${root}"/}"
        if [ ! -r "${file}" ]; then
            violations+="${rel}: cannot be read"$'\n'
            continue
        fi
        created="$(count "${file}" "${CREATE}")"
        removed="$(count "${file}" "${REMOVE}")"
        [ "${created}" -eq "${removed}" ] \
            || violations+="${rel} creates ${created} status items and removes ${removed}; remove each after its last use"$'\n'
    done < <(printf '%s\n' "${files}" | /usr/bin/sort)
    if [ -n "${violations}" ]; then
        printf '%s' "${violations}"
        return 1
    fi
}

# --- The checker rejects each kind of violation --------------------------------
make_fixture() {
    local dir="${TEMP_ROOT}/$1"
    /bin/mkdir -p "${dir}/tests"
    printf '%s\n' \
        '  // A comment may name NSStatusBar.system.statusItem(withLength: 1).' \
        '  let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)' \
        '  delegate.statusItem = item' \
        '  NSStatusBar.system.removeStatusItem(item)' > "${dir}/tests/AppDelegateTests.swift"
    echo "${dir}"
}
expect_violation() {
    # expect_violation NAME FILE LINE PATTERN: the fixture plus LINE in FILE is rejected with PATTERN
    local dir output
    dir="$(make_fixture "$1")"
    /bin/mkdir -p "$(dirname "${dir}/$2")"
    printf '%s\n' "$3" >> "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted $2 with: $3"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$4" || fail "$1: unexpected report: ${output}"
}

clean="$(make_fixture clean)"
output="$(check_tree "${clean}")" || fail "clean fixture was rejected: ${output}"

T=tests/AppDelegateTests.swift
expect_violation kept "${T}" \
    '  appDelegate.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)' \
    "${T} creates 2 status items and removes 1"
expect_violation aliased "${T}" '  let other = bar.statusItem(withLength: 24)' "${T} creates 2 status items and removes 1"
expect_violation spaced "${T}" '  let other = NSStatusBar.system.statusItem (withLength: 24)' \
    "${T} creates 2 status items and removes 1"
expect_violation split-call "${T}" '  let other = NSStatusBar.system.statusItem(' \
    "${T} creates 2 status items and removes 1"
expect_violation same-line "${T}" \
    '  let a = NSStatusBar.system.statusItem(withLength: 1); let b = NSStatusBar.system.statusItem(withLength: 2); NSStatusBar.system.removeStatusItem(a)' \
    "${T} creates 3 status items and removes 2"
expect_violation extra-removal "${T}" '  NSStatusBar.system.removeStatusItem(item)' \
    "${T} creates 1 status items and removes 2"
expect_violation subdirectory tests/support/Helpers.swift \
    '  let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)' \
    'tests/support/Helpers.swift creates 1 status items and removes 0'
# Counts are per file: one file's extra removal must not hide another file's leak.
split_files="$(make_fixture split-files)"
printf '%s\n' '  let other = NSStatusBar.system.statusItem(withLength: 24)' >> "${split_files}/${T}"
printf '%s\n' '  NSStatusBar.system.removeStatusItem(other)' > "${split_files}/tests/OtherTests.swift"
if output="$(check_tree "${split_files}")"; then
    fail "split-files: the checker accepted a leak offset by another file's extra removal"
fi
printf '%s\n' "${output}" | /usr/bin/grep -qF "${T} creates 2 status items and removes 1" \
    || fail "split-files: unexpected report: ${output}"
if [ "$(/usr/bin/id -u)" -ne 0 ]; then
    hidden="$(make_fixture unlistable)"
    /bin/mkdir -p "${hidden}/tests/sub"
    printf '%s\n' '  let item = NSStatusBar.system.statusItem(withLength: 24)' > "${hidden}/tests/sub/Bad.swift"
    /bin/chmod 000 "${hidden}/tests/sub"
    if output="$(check_tree "${hidden}" 2>/dev/null)"; then
        /bin/chmod 700 "${hidden}/tests/sub"
        fail "unlistable: the checker accepted a tree it could not list"
    fi
    /bin/chmod 700 "${hidden}/tests/sub"
    printf '%s\n' "${output}" | /usr/bin/grep -qF 'tests: cannot list every Swift test' \
        || fail "unlistable: unexpected report: ${output}"
    unreadable="$(make_fixture unreadable)"
    /bin/chmod 000 "${unreadable}/${T}"
    if output="$(check_tree "${unreadable}")"; then
        fail "unreadable: the checker accepted an unreadable test file"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF "${T}: cannot be read" \
        || fail "unreadable: unexpected report: ${output}"
fi

# --- The repository follows the rule ------------------------------------------
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests leave status items in the menu bar:
${output}"

echo "swift test status items: ok"
