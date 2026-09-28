#!/bin/bash
# Static check: Swift tests never use the standard defaults store.
#
# A Swift test binary has no bundle, so its standard defaults are the per-user
# domain named after the executable (`app_delegate_test`). Every concurrent run
# of the suite on the machine shares it, so one run's write could change what
# another run read. Tests build AppDelegate through its designated initializer
# with a tests/TestPreferencesSuite.swift store private to the run; in Sources/
# only the composition roots, AppDelegate() and AutoLaunchManager.shared, pick
# the standard store. See
# docs/leanings/2026-09-28-concurrent-swift-suites-isolated-by-injected-defaults.md.
#
# Rules, for every *.swift file under tests/ and Sources/ (lines that start
# with // are ignored; block comments, trailing comments, and strings are not):
#   tests   - no standard store in any spelling (including `UserDefaults()`,
#             `suiteName: nil`, `@AppStorage`, and any `.standard` shorthand;
#             spell another type's member as `Type.standard`), no CFPreferences
#             or `defaults` tool, no `AppDelegate()`, and every AutoLaunchManager
#             is built with `userDefaults:` on the same line. Suites are created
#             and removed only in TestPreferencesSuite.swift. The one exception
#             is the pair of identity checks that prove AppDelegate() keeps the
#             standard store and shared login items; both must be present.
#   Sources - the standard store appears once in AppDelegate.swift (the
#             `defaults:` argument of AppDelegate()) and once in
#             AutoLaunchManager.swift (its default argument), nowhere else, and
#             only AutoLaunchManager.shared builds a login-item manager without
#             `userDefaults:`.
# scripts/*.swift helpers are not scanned: they are separate programs, and the
# one that reads defaults only reads ChatGPT's and the global domain.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-defaults-lint.XXXXXX")"
trap '/bin/chmod -R u+rw -- "${TEMP_ROOT}" 2>/dev/null || true; /bin/rm -rf -- "${TEMP_ROOT}"' EXIT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

S='[[:space:]]*'
INIT="(\\.${S}init${S})?"
# `UserDefaults()` and `UserDefaults(suiteName: nil)` search the app's own
# domain like the standard store, and `@AppStorage` defaults to it. A line that
# starts with `.standard` continues a member chain split across lines.
STANDARD_STORE="UserDefaults${S}\\.${S}standard|UserDefaults${S}=${S}\\.standard|UserDefaults${S}${INIT}\\(${S}\\)|suiteName${S}:${S}nil|^${S}\\.${S}standard([^[:alnum:]_]|\$)|standardUserDefaults|NSUserDefaults|CFPreferences|@AppStorage"
TEST_ONLY_FORBIDDEN="${STANDARD_STORE}|(^|[^[:alnum:]_])\\.standard([^[:alnum:]_]|\$)|AppDelegate${S}${INIT}\\(${S}\\)|(^|[^[:alnum:]_])\\.init${S}\\(${S}\\)|AutoLaunchManager${S}\\.${S}shared|/usr/bin/defaults"
SUITE_LIFECYCLE="UserDefaults${S}${INIT}\\(${S}suiteName|(^|[^[:alnum:]_])\\.init${S}\\(${S}suiteName|PersistentDomain${S}\\("
LOGIN_ITEMS_BUILT="AutoLaunchManager${S}${INIT}\\(|AutoLaunchManager${S}=${S}\\.init"
# The production wiring checks, spelled exactly; nothing else may follow them.
WIRING_DEFAULTS="${S}assertTrue\\(AppDelegate\\(\\)\\.defaults === UserDefaults\\.standard, \"[^\"]*\"\\)\$"
WIRING_LOGIN_ITEMS="${S}assertTrue\\(AppDelegate\\(\\)\\.autoLaunchManager === AutoLaunchManager\\.shared, \"[^\"]*\"\\)\$"
# A composition root passes the standard store as an argument, not a receiver.
APP_DELEGATE_ROOT="defaults:${S}UserDefaults\\.standard([^[:alnum:]_.]|\$)"
LOGIN_ITEMS_ROOT="userDefaults:${S}UserDefaults${S}=${S}\\.standard([^[:alnum:]_.]|\$)"
SHARED_LOGIN_ITEMS="static let shared = AutoLaunchManager\\(\\)"

# code_matches FILE ERE -> "file:line:text" for each matching line that is not
# a // comment. A file grep cannot read is reported as a match, so it fails.
code_matches() {
    local out status=0
    out="$(/usr/bin/grep -n -E -- "$2" "$1" 2>/dev/null)" || status=$?
    if [ "${status}" -gt 1 ]; then
        printf '%s: cannot be read\n' "${1#"${ROOT}"/}"
        return 0
    fi
    [ -n "${out}" ] || return 0
    printf '%s\n' "${out}" \
        | /usr/bin/grep -v -E '^[0-9]+:[[:space:]]*//' \
        | /usr/bin/sed "s#^#${1#"${ROOT}"/}:#" || true
}

# without LINES ERE -> the "file:line:text" LINES whose text does not match ^ERE
without() {
    [ -n "$1" ] || return 0
    printf '%s\n' "$1" | /usr/bin/grep -v -E -- "^[^:]+:[0-9]+:$2" || true
}

# count_matching LINES ERE -> how many of LINES match ERE
count_matching() {
    [ -n "$1" ] || { echo 0; return 0; }
    printf '%s\n' "$1" | /usr/bin/grep -c -E -- "$2" || true
}

# check_tree ROOT -> prints each violation; returns 1 if there is any
check_tree() {
    ROOT="$1"
    local violations="" file rel found count root wiring_defaults=0 wiring_login_items=0
    while IFS= read -r file; do
        rel="${file#"${ROOT}"/}"
        [ -z "$(code_matches "${file}" "^${WIRING_DEFAULTS}")" ] || wiring_defaults=1
        [ -z "$(code_matches "${file}" "^${WIRING_LOGIN_ITEMS}")" ] || wiring_login_items=1
        found="$(code_matches "${file}" "${TEST_ONLY_FORBIDDEN}")"
        found="$(without "${found}" "${WIRING_DEFAULTS}")"
        found="$(without "${found}" "${WIRING_LOGIN_ITEMS}")"
        [ -z "${found}" ] || violations+="${found}"$'\n'
        case "${rel}" in
            tests/TestPreferencesSuite.swift) ;;
            *)
                found="$(code_matches "${file}" "${SUITE_LIFECYCLE}")"
                [ -z "${found}" ] || violations+="${found} (create and remove suites only in TestPreferencesSuite.swift)"$'\n'
                ;;
        esac
        found="$(without "$(code_matches "${file}" "${LOGIN_ITEMS_BUILT}")" '.*userDefaults:')"
        [ -z "${found}" ] || violations+="${found} (build AutoLaunchManager( with userDefaults: on the same line)"$'\n'
    done < <(/usr/bin/find "${ROOT}/tests" -type f -name '*.swift' | /usr/bin/sort)
    [ "${wiring_defaults}" -eq 1 ] \
        || violations+="tests: no check that AppDelegate() uses UserDefaults.standard"$'\n'
    [ "${wiring_login_items}" -eq 1 ] \
        || violations+="tests: no check that AppDelegate() uses AutoLaunchManager.shared"$'\n'

    for rel in Sources/AppDelegate.swift Sources/AutoLaunchManager.swift; do
        [ -f "${ROOT}/${rel}" ] || violations+="${rel} is missing; it holds a composition root"$'\n'
    done
    while IFS= read -r file; do
        rel="${file#"${ROOT}"/}"
        found="$(code_matches "${file}" "${STANDARD_STORE}")"
        case "${rel}" in
            Sources/AppDelegate.swift | Sources/AutoLaunchManager.swift)
                root="${LOGIN_ITEMS_ROOT}"
                [ "${rel}" != Sources/AppDelegate.swift ] || root="${APP_DELEGATE_ROOT}"
                count="$(count_matching "${found}" "${root}")"
                [ "${count}" -eq 1 ] \
                    || violations+="${rel} passes the standard store ${count} times; its composition root must pass it once"$'\n'
                found="$(without "${found}" ".*${root}")"
                ;;
        esac
        [ -z "${found}" ] \
            || violations+="${found} (inject the store; only AppDelegate() and AutoLaunchManager.shared choose the standard one)"$'\n'
        found="$(without "$(code_matches "${file}" "${LOGIN_ITEMS_BUILT}")" '.*userDefaults:')"
        if [ "${rel}" = Sources/AutoLaunchManager.swift ]; then
            count="$(count_matching "${found}" "${SHARED_LOGIN_ITEMS}")"
            [ "${count}" -le 1 ] || violations+="${rel} builds the shared login items ${count} times"$'\n'
            found="$(without "${found}" ".*${SHARED_LOGIN_ITEMS}")"
        fi
        [ -z "${found}" ] \
            || violations+="${found} (pass userDefaults:; only AutoLaunchManager.shared uses the default store)"$'\n'
    done < <(/usr/bin/find "${ROOT}/Sources" -type f -name '*.swift' | /usr/bin/sort)
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
        '  assertTrue(AppDelegate().defaults === UserDefaults.standard, "standard store")' \
        '  assertTrue(AppDelegate().autoLaunchManager === AutoLaunchManager.shared, "shared login items")' \
        > "${dir}/tests/AppDelegateWiringTests.swift"
    printf '%s\n' \
        'self.init(client: CodexClient.shared, defaults: UserDefaults.standard, autoLaunchManager: AutoLaunchManager.shared)' \
        'let stacked = defaults.object(forKey: AppDelegate.stackPercentagesKey)' > "${dir}/Sources/AppDelegate.swift"
    printf '%s\n' \
        '  public static let shared = AutoLaunchManager()' \
        '    userDefaults: UserDefaults = .standard,' > "${dir}/Sources/AutoLaunchManager.swift"
    printf '%s\n' 'let stacked = stacksPercentages' > "${dir}/Sources/AppDelegate+Menu.swift"
    echo "${dir}"
}
expect_violation() {
    # expect_violation NAME FILE LINE PATTERN
    local dir output
    dir="$(make_fixture "$1")"
    /bin/mkdir -p "$(dirname "${dir}/$2")"
    printf '%s\n' "$3" >> "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted: $3"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$4" || fail "$1: unexpected report: ${output}"
}
expect_replaced() {
    # expect_replaced NAME FILE CONTENT PATTERN: the fixture with FILE replaced is rejected
    local dir output
    dir="$(make_fixture "$1")"
    printf '%s\n' "$3" > "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted $2 as: $3"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$4" || fail "$1: unexpected report: ${output}"
}
expect_missing() {
    # expect_missing NAME FILE PATTERN: the fixture without FILE is rejected
    local dir output
    dir="$(make_fixture "$1")"
    /bin/rm -f "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted a tree without $2"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$3" || fail "$1: unexpected report: ${output}"
}

clean="$(make_fixture clean)"
output="$(check_tree "${clean}")" || fail "clean fixture was rejected: ${output}"

T=tests/AppDelegateTests.swift
expect_violation test-standard-read "${T}" \
    'let stacked = UserDefaults.standard.bool(forKey: "stackPercentages")' "${T}:4:"
expect_violation test-standard-spaced "${T}" 'let d = UserDefaults .standard' "${T}:4:"
expect_violation test-standard-split "${T}" '  .standard.set(false, forKey: "stackPercentages")' "${T}:4:"
expect_violation test-standard-shorthand "${T}" \
    'let delegate = AppDelegate(client: client, defaults: .standard, autoLaunchManager: manager)' 'defaults: .standard'
expect_violation test-default-init "${T}" 'let store = UserDefaults()' 'UserDefaults()'
expect_violation test-default-init-explicit "${T}" 'let store = UserDefaults.init()' 'UserDefaults.init()'
expect_violation test-nil-suite "${T}" 'let store = UserDefaults(suiteName: nil)!' 'suiteName: nil'
expect_violation test-app-storage "${T}" '@AppStorage("stackPercentages") var stacked = true' '@AppStorage'
expect_violation test-cfpreferences "${T}" 'CFPreferencesSetAppValue(key, value, domain)' 'CFPreferencesSetAppValue'
expect_violation test-production-delegate "${T}" 'let delegate = AppDelegate()' 'AppDelegate()'
expect_violation test-production-delegate-init "${T}" 'let delegate = AppDelegate.init()' 'AppDelegate.init()'
expect_violation test-production-delegate-shorthand "${T}" 'let delegate: AppDelegate = .init()' '.init()'
expect_violation test-wiring-line-with-write "${T}" \
    '  assertTrue(AppDelegate().defaults === UserDefaults.standard, "x"); UserDefaults.standard.set(1, forKey: "k")' "${T}:4:"
expect_violation test-shared-login-items "${T}" 'let manager = AutoLaunchManager.shared' 'AutoLaunchManager.shared'
expect_violation test-default-login-items "${T}" 'let manager = AutoLaunchManager(scriptExecutor: s)' 'userDefaults: on the same line'
expect_violation test-default-login-items-init "${T}" \
    'let manager: AutoLaunchManager = .init(scriptExecutor: s)' 'userDefaults: on the same line'
expect_violation test-defaults-tool "${T}" \
    'process.executableURL = URL(fileURLWithPath: "/usr/bin/defaults")' '/usr/bin/defaults'
expect_violation test-foreign-suite "${T}" \
    'let monitor = UserDefaults(suiteName: "com.codex.monitor")' 'only in TestPreferencesSuite.swift'
expect_violation test-foreign-suite-init "${T}" \
    'let monitor = UserDefaults.init(suiteName: "com.codex.monitor")' 'only in TestPreferencesSuite.swift'
expect_violation test-foreign-suite-shorthand "${T}" \
    'let monitor: UserDefaults? = .init(suiteName: "com.codex.monitor")' 'only in TestPreferencesSuite.swift'
expect_violation test-foreign-domain-removal "${T}" \
    'suite.removePersistentDomain(forName: "com.codex.monitor")' 'only in TestPreferencesSuite.swift'
expect_violation test-in-subdirectory tests/support/Helpers.swift \
    'let stacked = UserDefaults.standard.bool(forKey: "stackPercentages")' 'tests/support/Helpers.swift:1:'
expect_missing test-no-wiring-check tests/AppDelegateWiringTests.swift 'no check that AppDelegate() uses UserDefaults.standard'

M=Sources/AppDelegate+Menu.swift
expect_violation source-extension-read "${M}" \
    'let isStacked = UserDefaults.standard.object(forKey: "stackPercentages")' "${M}:2:"
expect_violation source-second-root-read Sources/AppDelegate.swift \
    'let saved = UserDefaults.standard.double(forKey: AppDelegate.refreshIntervalKey)' 'Sources/AppDelegate.swift:3:'
expect_violation source-new-default-arg "${M}" 'init(store: UserDefaults = .standard) {}' "${M}:2:"
expect_violation source-default-init "${M}" 'let store = UserDefaults()' "${M}:2:"
expect_violation source-nil-suite "${M}" 'let store = UserDefaults(suiteName: nil)!' "${M}:2:"
expect_violation source-default-login-items "${M}" 'let items = AutoLaunchManager()' "${M}:2:"
expect_violation source-second-shared-login-items Sources/AutoLaunchManager.swift \
    '  public static let other = AutoLaunchManager()' 'Sources/AutoLaunchManager.swift:3:'
expect_violation source-root-reader Sources/AutoLaunchManager.swift \
    'return UserDefaults.standard.bool(forKey: Self.userDefaultsKey)' 'Sources/AutoLaunchManager.swift:3:'
expect_violation source-second-root-argument Sources/AppDelegate.swift \
    'self.init(client: CodexClient.shared, defaults: UserDefaults.standard, autoLaunchManager: AutoLaunchManager.shared)' \
    'passes the standard store 2 times'
expect_replaced source-lost-root Sources/AppDelegate.swift \
    'self.init(client: CodexClient.shared, defaults: UserDefaults(suiteName: "x")!, autoLaunchManager: AutoLaunchManager.shared)' \
    'Sources/AppDelegate.swift passes the standard store 0 times'
expect_replaced source-lost-login-root Sources/AutoLaunchManager.swift \
    '    userDefaults: UserDefaults,' 'Sources/AutoLaunchManager.swift passes the standard store 0 times'
expect_missing source-missing-root Sources/AppDelegate.swift 'Sources/AppDelegate.swift is missing'

# An unreadable file fails the check instead of passing it.
if [ "$(/usr/bin/id -u)" -ne 0 ]; then
    unreadable="$(make_fixture unreadable)"
    /bin/chmod 000 "${unreadable}/tests/AppDelegateTests.swift"
    if output="$(check_tree "${unreadable}")"; then
        fail "unreadable: the checker accepted an unreadable test file"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF 'tests/AppDelegateTests.swift: cannot be read' \
        || fail "unreadable: unexpected report: ${output}"
fi

# --- The repository follows the rules -----------------------------------------
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests or sources use the standard defaults store:
${output}"

echo "swift test defaults isolation: ok"
