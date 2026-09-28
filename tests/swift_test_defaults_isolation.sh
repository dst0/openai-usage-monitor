#!/bin/bash
# Static check: Swift tests never use the standard defaults store.
#
# A Swift test binary has no bundle, so its standard defaults are the per-user
# domain named after the executable (`app_delegate_test`). Every concurrent run
# of the suite on the machine shares it, so one run's write could change what
# another run read. Tests build AppDelegate through its designated initializer
# with a tests/TestPreferencesSuite.swift store private to the run; only the
# app's composition roots pick the standard store. See
# docs/leanings/2026-09-28-concurrent-swift-suites-isolated-by-injected-defaults.md.
#
# Rules (comment lines are ignored):
#   tests/*.swift   - no standard store, no CFPreferences or `defaults` tool,
#                     no AppDelegate() or default AutoLaunchManager; a
#                     suite is created and removed only in TestPreferencesSuite.swift.
#   Sources/*.swift - the standard store appears only on one line each in
#                     AppDelegate.swift (AppDelegate()) and AutoLaunchManager.swift
#                     (its default argument).
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-defaults-lint.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

STANDARD_STORE='UserDefaults[[:space:]]*\.[[:space:]]*standard|UserDefaults[[:space:]]*=[[:space:]]*\.standard|standardUserDefaults|NSUserDefaults|CFPreferences'
TEST_ONLY_FORBIDDEN="${STANDARD_STORE}"'|AppDelegate[[:space:]]*\([[:space:]]*\)|AutoLaunchManager[[:space:]]*\.[[:space:]]*shared|AutoLaunchManager[[:space:]]*\([[:space:]]*\)|/usr/bin/defaults'
SUITE_LIFECYCLE='UserDefaults[[:space:]]*\([[:space:]]*suiteName|PersistentDomain[[:space:]]*\('

# code_matches FILE ERE -> "file:line: text" for each non-comment matching line
code_matches() {
    /usr/bin/grep -n -E -- "$2" "$1" \
        | /usr/bin/grep -v -E '^[0-9]+:[[:space:]]*//' \
        | /usr/bin/sed "s#^#${1#"${ROOT}"/}:#" || true
}

# check_tree ROOT -> prints each violation; returns 1 if there is any
check_tree() {
    ROOT="$1"
    local violations="" file found count
    for file in "${ROOT}"/tests/*.swift; do
        [ -f "${file}" ] || continue
        found="$(code_matches "${file}" "${TEST_ONLY_FORBIDDEN}")"
        [ -z "${found}" ] || violations+="${found}"$'\n'
        if [ "${file##*/}" != TestPreferencesSuite.swift ]; then
            found="$(code_matches "${file}" "${SUITE_LIFECYCLE}")"
            [ -z "${found}" ] || violations+="${found} (create and remove suites only in TestPreferencesSuite.swift)"$'\n'
        fi
        found="$(code_matches "${file}" 'AutoLaunchManager[[:space:]]*\(' | /usr/bin/grep -v 'userDefaults:' || true)"
        [ -z "${found}" ] || violations+="${found} (pass userDefaults: on the same line)"$'\n'
    done
    for file in "${ROOT}"/Sources/*.swift; do
        [ -f "${file}" ] || continue
        found="$(code_matches "${file}" "${STANDARD_STORE}")"
        [ -n "${found}" ] || continue
        # Each composition root may pass the standard store once, in its argument line.
        case "${file##*/}" in
            AppDelegate.swift) root_line='defaults:[[:space:]]*UserDefaults\.standard,?[[:space:]]*$' ;;
            AutoLaunchManager.swift) root_line='userDefaults:[[:space:]]*UserDefaults[[:space:]]*=[[:space:]]*\.standard,?[[:space:]]*$' ;;
            *) root_line='' ;;
        esac
        if [ -n "${root_line}" ]; then
            count="$(printf '%s\n' "${found}" | /usr/bin/grep -c -E -- "${root_line}" || true)"
            [ "${count}" -le 1 ] || violations+="${file##*/} passes the standard store ${count} times"$'\n'
            found="$(printf '%s\n' "${found}" | /usr/bin/grep -v -E -- "${root_line}" || true)"
        fi
        [ -z "${found}" ] \
            || violations+="${found} (inject the store; only AppDelegate() and AutoLaunchManager.shared choose the standard one)"$'\n'
    done
    if [ -n "${violations}" ]; then
        printf '%s' "${violations}"
        return 1
    fi
}

# --- The checker rejects each kind of violation --------------------------------
make_fixture() {
    local dir="${TEMP_ROOT}/$1"
    /bin/mkdir -p "${dir}/tests" "${dir}/Sources"
    printf '%s\n' \
        'let suite = UserDefaults(suiteName: name)!' \
        'suite.removePersistentDomain(forName: name)' > "${dir}/tests/TestPreferencesSuite.swift"
    printf '%s\n' \
        '// Never touch UserDefaults.standard here.' \
        'let delegate = AppDelegate(client: client, defaults: suite, autoLaunchManager: manager)' \
        'let manager = AutoLaunchManager(scriptExecutor: s, smService: m, userDefaults: suite)' \
        > "${dir}/tests/AppDelegateTests.swift"
    printf '%s\n' \
        '      client: CodexClient.shared, defaults: UserDefaults.standard,' \
        'let stacked = defaults.object(forKey: AppDelegate.stackPercentagesKey)' > "${dir}/Sources/AppDelegate.swift"
    printf '%s\n' '    userDefaults: UserDefaults = .standard,' > "${dir}/Sources/AutoLaunchManager.swift"
    printf '%s\n' 'let stacked = stacksPercentages' > "${dir}/Sources/AppDelegate+Menu.swift"
    echo "${dir}"
}
expect_violation() {
    # expect_violation NAME FILE LINE PATTERN
    local dir output
    dir="$(make_fixture "$1")"
    printf '%s\n' "$3" >> "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted: $3"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$4" || fail "$1: unexpected report: ${output}"
}

clean="$(make_fixture clean)"
output="$(check_tree "${clean}")" || fail "clean fixture was rejected: ${output}"

expect_violation test-standard-read tests/AppDelegateTests.swift \
    'let stacked = UserDefaults.standard.bool(forKey: "stackPercentages")' 'tests/AppDelegateTests.swift:4:'
expect_violation test-standard-spaced tests/AppDelegateTests.swift \
    'let d = UserDefaults .standard' 'tests/AppDelegateTests.swift:4:'
expect_violation test-cfpreferences tests/AppDelegateTests.swift \
    'CFPreferencesSetAppValue(key, value, domain)' 'CFPreferencesSetAppValue'
expect_violation test-production-delegate tests/AppDelegateTests.swift \
    'let delegate = AppDelegate()' 'AppDelegate()'
expect_violation test-shared-login-items tests/AppDelegateTests.swift \
    'let manager = AutoLaunchManager.shared' 'AutoLaunchManager.shared'
expect_violation test-default-login-items tests/AppDelegateTests.swift \
    'let manager = AutoLaunchManager(scriptExecutor: s)' 'pass userDefaults:'
expect_violation test-defaults-tool tests/AppDelegateTests.swift \
    'process.executableURL = URL(fileURLWithPath: "/usr/bin/defaults")' '/usr/bin/defaults'
expect_violation test-foreign-suite tests/AppDelegateTests.swift \
    'let monitor = UserDefaults(suiteName: "com.codex.monitor")' 'only in TestPreferencesSuite.swift'
expect_violation test-foreign-domain-removal tests/AppDelegateTests.swift \
    'suite.removePersistentDomain(forName: "com.codex.monitor")' 'only in TestPreferencesSuite.swift'
expect_violation source-extension-read Sources/AppDelegate+Menu.swift \
    'let isStacked = UserDefaults.standard.object(forKey: "stackPercentages")' 'Sources/AppDelegate+Menu.swift:2:'
expect_violation source-second-root-read Sources/AppDelegate.swift \
    'let saved = UserDefaults.standard.double(forKey: AppDelegate.refreshIntervalKey)' 'Sources/AppDelegate.swift:3:'
expect_violation source-new-default-arg Sources/AppDelegate+Menu.swift \
    'init(store: UserDefaults = .standard) {}' 'Sources/AppDelegate+Menu.swift:2:'
expect_violation source-root-reader Sources/AutoLaunchManager.swift \
    'return UserDefaults.standard.bool(forKey: Self.userDefaultsKey)' 'Sources/AutoLaunchManager.swift:2:'
expect_violation source-second-root-argument Sources/AppDelegate.swift \
    '      client: CodexClient.shared, defaults: UserDefaults.standard,' 'passes the standard store 2 times'

# --- The repository follows the rules -----------------------------------------
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests or sources use the standard defaults store:
${output}"

echo "swift test defaults isolation: ok"
