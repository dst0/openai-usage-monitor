#!/bin/bash
# Static check: Swift tests never check the installed Monitor CLI or read the live
# process list or window list.
#
# CodexClient finds the installed codex-mon (~/.local/bin, then a ~/dev checkout)
# and asks whether ChatGPT is running (the running-application list and
# proc_pidinfo); AppDelegate reads every app's on-screen windows to save Desktop
# bounds. Tests used to reach all three: Test 7 built a distribution process on
# the installed CLI, every loadCachedSnapshot() in the App/CLI identity tests
# asked whether ChatGPT was running, and the window-bounds test read the window
# list. They are now injected: only the composition roots CodexClient() and
# AppDelegate() bind the live lookups, without calling them, and tests pass
# fakes (tests/TestCodexHome.swift, makeTestAppDelegate). To look at the live
# values on purpose, run scripts/swift_live_diagnostics.sh --allow-live-system;
# no test or CI step runs it. See
# docs/leanings/2026-09-28-swift-tests-read-the-installed-cli-and-process-list.md.
#
# Rules (lines whose first non-blank characters are // are ignored):
#   Sources - the installed-CLI lookup, CodexDesktopProcessIdentity.current, and
#             the live window list are each named only by their declaration and
#             by one binding line in their composition root, spelled exactly,
#             without a call; CodexClient has no static cliExecutableURL; a
#             ~/.local/bin path appears only in the installed-CLI lookup and in
#             refreshCLIVersion (which only the app's launch runs); the window
#             list is read only in liveDesktopWindows among the AppDelegate files.
#   tests   - no name of a live lookup or of a path that reaches one: the three
#             above, NSRunningApplication, runningApplications, proc_pidinfo,
#             CodexRecoveryProcessIdentity.birth, CGWindowListCopyWindowInfo,
#             isAnotherInstanceRunning, refreshCLIVersion,
#             applicationDidFinishLaunching, start/stopBackgroundAutomation,
#             quitApp (both run launchctl), a .local/bin path, or the
#             diagnostic script.
#   scripts/test_swift.sh and the workflows never name the diagnostic script.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-live-lint.XXXXXX")"
trap '/bin/chmod -R u+rw -- "${TEMP_ROOT}" 2>/dev/null || true; /bin/rm -rf -- "${TEMP_ROOT}"' EXIT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

S='[[:space:]]*'
CLI_LOOKUP="installedCLIExecutable"
CLI_DECLARED="^${S}internal static func installedCLIExecutable\\(\\) -> URL \\{\$"
CLI_BOUND="^${S}let cli = Self\\.installedCLIExecutable\$"
DESKTOP_LOOKUP="CodexDesktopProcessIdentity${S}\\.${S}current"
DESKTOP_BOUND="^${S}let desktop = CodexDesktopProcessIdentity\\.current\$"
WINDOWS_LOOKUP="liveDesktopWindows"
WINDOWS_DECLARED="^${S}internal static func liveDesktopWindows\\(\\) -> \\[\\[String: Any\\]\\] \\{\$"
WINDOWS_BOUND="^${S}autoLaunchManager: AutoLaunchManager\\.shared, desktopWindows: AppDelegate\\.liveDesktopWindows\\)\$"
STATIC_CLI_URL="static${S}var${S}cliExecutableURL|(Self|CodexClient)${S}\\.${S}cliExecutableURL"
LOCAL_BIN="\\.local/bin"
WINDOW_LIST="CGWindowListCopyWindowInfo"
DIAGNOSTIC="swift_live_diagnostics"
TEST_FORBIDDEN="${CLI_LOOKUP}|${DESKTOP_LOOKUP}|${WINDOWS_LOOKUP}|NSRunningApplication|runningApplications|proc_pidinfo|CodexRecoveryProcessIdentity${S}\\.${S}birth|${WINDOW_LIST}|isAnotherInstanceRunning|refreshCLIVersion|applicationDidFinishLaunching|startBackgroundAutomation|stopBackgroundAutomation|quitApp|${LOCAL_BIN}|${DIAGNOSTIC}"

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

# expect_only LABEL MATCHES ALLOWED... -> a violation unless each ALLOWED ERE (a
# whole "file:line:text" pattern) matches exactly one of MATCHES and every line of
# MATCHES matches one of them.
expect_only() {
    local label="$1" matches="$2" line allowed found
    shift 2
    for allowed in "$@"; do
        found="$(printf '%s\n' "${matches}" | /usr/bin/grep -c -E -- "^${allowed}" || true)"
        [ "${found}" -eq 1 ] || printf '%s must be named exactly once by a line matching %s, found %s\n' \
            "${label}" "${allowed}" "${found}"
    done
    while IFS= read -r line; do
        [ -n "${line}" ] || continue
        found=0
        for allowed in "$@"; do
            printf '%s\n' "${line}" | /usr/bin/grep -q -E -- "^${allowed}" && found=1
        done
        [ "${found}" -eq 1 ] || printf '%s (%s is named only where the rule allows)\n' "${line}" "${label}"
    done <<< "${matches}"
}

# check_tree ROOT -> prints each violation; returns 1 if there is any
check_tree() {
    ROOT="$1"
    local violations="" file rel found all
    local cli_all="" desktop_all="" windows_all="" local_bin_all="" window_list_all=""
    [ -d "${ROOT}/Sources" ] && [ -d "${ROOT}/tests" ] || { printf 'Sources or tests is missing under %s\n' "${ROOT}"; return 1; }
    all="$(/usr/bin/find "${ROOT}/Sources" -type f -name '*.swift')" || { printf 'cannot list Sources\n'; return 1; }
    while IFS= read -r file; do
        [ -n "${file}" ] || continue
        rel="${file#"${ROOT}"/}"
        cli_all+="$(code_matches "${file}" "${CLI_LOOKUP}")"$'\n'
        desktop_all+="$(code_matches "${file}" "${DESKTOP_LOOKUP}")"$'\n'
        windows_all+="$(code_matches "${file}" "${WINDOWS_LOOKUP}")"$'\n'
        local_bin_all+="$(code_matches "${file}" "${LOCAL_BIN}")"$'\n'
        case "${rel}" in
            Sources/AppDelegate*.swift) window_list_all+="$(code_matches "${file}" "${WINDOW_LIST}")"$'\n' ;;
        esac
        found="$(code_matches "${file}" "${STATIC_CLI_URL}")"
        [ -z "${found}" ] || violations+="${found} (CodexClient takes its CLI executable; no static lookup)"$'\n'
    done < <(printf '%s\n' "${all}" | /usr/bin/sort)
    violations+="$(expect_only 'the installed-CLI lookup' "$(printf '%s' "${cli_all}" | /usr/bin/sed '/^$/d')" \
        "Sources/CodexClient.swift:[0-9]+:${CLI_DECLARED#^}" "Sources/CodexClient.swift:[0-9]+:${CLI_BOUND#^}")"$'\n'
    violations+="$(expect_only 'CodexDesktopProcessIdentity.current' "$(printf '%s' "${desktop_all}" | /usr/bin/sed '/^$/d')" \
        "Sources/CodexClient.swift:[0-9]+:${DESKTOP_BOUND#^}")"$'\n'
    violations+="$(expect_only 'the live window list' "$(printf '%s' "${windows_all}" | /usr/bin/sed '/^$/d')" \
        "Sources/AppDelegate\\+WindowBounds.swift:[0-9]+:${WINDOWS_DECLARED#^}" \
        "Sources/AppDelegate.swift:[0-9]+:${WINDOWS_BOUND#^}")"$'\n'
    violations+="$(expect_only 'a ~/.local/bin path' "$(printf '%s' "${local_bin_all}" | /usr/bin/sed '/^$/d')" \
        'Sources/CodexClient.swift:[0-9]+:.*"\.local/bin/codex-mon"' \
        'Sources/AppDelegate\+SettingsActions.swift:[0-9]+:.*/\.local/bin/cxi"' \
        'Sources/AppDelegate\+SettingsActions.swift:[0-9]+:.*/\.local/bin/codex-mon"')"$'\n'
    violations+="$(expect_only 'the window list' "$(printf '%s' "${window_list_all}" | /usr/bin/sed '/^$/d')" \
        "Sources/AppDelegate\\+WindowBounds.swift:[0-9]+:.*${WINDOW_LIST}")"$'\n'

    all="$(/usr/bin/find "${ROOT}/tests" -type f -name '*.swift')" || { printf 'cannot list tests\n'; return 1; }
    while IFS= read -r file; do
        [ -n "${file}" ] || continue
        found="$(code_matches "${file}" "${TEST_FORBIDDEN}")"
        [ -z "${found}" ] || violations+="${found} (tests must not reach the live CLI, process list, window list, or launchctl)"$'\n'
    done < <(printf '%s\n' "${all}" | /usr/bin/sort)
    for file in "${ROOT}/scripts/test_swift.sh" "${ROOT}"/.github/workflows/*.yml; do
        [ -f "${file}" ] || continue
        found="$(code_matches "${file}" "${DIAGNOSTIC}")"
        [ -z "${found}" ] || violations+="${found} (the live diagnostic runs only when asked for)"$'\n'
    done
    violations="$(printf '%s' "${violations}" | /usr/bin/sed '/^$/d')"
    if [ -n "${violations}" ]; then
        printf '%s\n' "${violations}"
        return 1
    fi
}

# --- The checker rejects each kind of violation --------------------------------
make_fixture() {
    local dir="${TEMP_ROOT}/$1"
    /bin/mkdir -p "${dir}/Sources" "${dir}/tests" "${dir}/scripts" "${dir}/.github/workflows"
    printf '%s\n' \
        '  internal static func installedCLIExecutable() -> URL {' \
        '    let localBin = home.appendingPathComponent(".local/bin/codex-mon")' \
        '  public convenience init() {' \
        '    let cli = Self.installedCLIExecutable' \
        '    let desktop = CodexDesktopProcessIdentity.current' \
        '  public var cliExecutableURL: URL { cliExecutable() }' > "${dir}/Sources/CodexClient.swift"
    printf '%s\n' \
        '      client: CodexClient.shared, defaults: UserDefaults.standard,' \
        '      autoLaunchManager: AutoLaunchManager.shared, desktopWindows: AppDelegate.liveDesktopWindows)' \
        > "${dir}/Sources/AppDelegate.swift"
    printf '%s\n' \
        '  internal static func liveDesktopWindows() -> [[String: Any]] {' \
        '    CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []' \
        > "${dir}/Sources/AppDelegate+WindowBounds.swift"
    printf '%s\n' \
        '        "\(NSHomeDirectory())/.local/bin/cxi",' \
        '        "\(NSHomeDirectory())/.local/bin/codex-mon",' > "${dir}/Sources/AppDelegate+SettingsActions.swift"
    printf '%s\n' \
        '    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID)' \
        > "${dir}/Sources/CodexRecoveryBanner.swift"
    printf '%s\n' \
        '  // Never call CodexDesktopProcessIdentity.current() from a test.' \
        '  let client = home.client(desktopProcess: { fakeDesktop })' > "${dir}/tests/AppDelegateTests.swift"
    printf '%s\n' 'swiftc -parse-as-library tests/AppDelegateTests.swift' > "${dir}/scripts/test_swift.sh"
    printf '%s\n' '        run: bash tests/swift_test_live_system_isolation.sh' > "${dir}/.github/workflows/ci.yml"
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
expect_replaced() {
    # expect_replaced NAME FILE CONTENT PATTERN: the fixture with FILE replaced is rejected with PATTERN
    local dir output
    dir="$(make_fixture "$1")"
    printf '%s\n' "$3" > "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted $2 as: $3"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$4" || fail "$1: unexpected report: ${output}"
}

clean="$(make_fixture clean)"
output="$(check_tree "${clean}")" || fail "clean fixture was rejected: ${output}"

C=Sources/CodexClient.swift
expect_violation source-static-cli "${C}" '  public static var cliExecutableURL: URL {' 'no static lookup'
expect_violation source-self-cli "${C}" '    let bin = Self.cliExecutableURL.path' 'no static lookup'
expect_violation source-desktop-call "${C}" '    CodexDesktopProcessIdentity.current() != nil' \
    'CodexDesktopProcessIdentity.current is named only where the rule allows'
expect_violation source-called-binding "${C}" '    let desktop = CodexDesktopProcessIdentity.current()' \
    'CodexDesktopProcessIdentity.current is named only where the rule allows'
expect_violation source-cli-call "${C}" '    process.executableURL = Self.installedCLIExecutable()' \
    'the installed-CLI lookup is named only where the rule allows'
expect_replaced source-cli-unbound "${C}" '  internal static func installedCLIExecutable() -> URL {' \
    'the installed-CLI lookup must be named exactly once by a line matching'
expect_violation source-other-file Sources/AccountRowView.swift \
    '      if CodexDesktopProcessIdentity.current() == nil {' 'Sources/AccountRowView.swift:1:'
expect_violation source-local-bin Sources/AppDelegate.swift \
    '    let helper = home.appendingPathComponent(".local/bin/codex-ui-resume")' 'a ~/.local/bin path is named only'
expect_violation source-window-list Sources/AppDelegate+StatusBar.swift \
    '    let list = CGWindowListCopyWindowInfo([], kCGNullWindowID)' 'the window list is named only'
expect_violation source-window-call Sources/AppDelegate+WindowBounds.swift \
    '    let list = AppDelegate.liveDesktopWindows()' 'the live window list is named only'
expect_replaced source-window-unbound Sources/AppDelegate.swift \
    '      autoLaunchManager: AutoLaunchManager.shared)' 'the live window list must be named exactly once'

T=tests/AppDelegateTests.swift
expect_violation test-cli "${T}" '  let url = CodexClient.installedCLIExecutable()' "${T}:3:"
expect_violation test-desktop "${T}" '  let running = CodexDesktopProcessIdentity .current() != nil' "${T}:3:"
expect_violation test-apps "${T}" '  let apps = NSWorkspace.shared.runningApplications' "${T}:3:"
expect_violation test-proc "${T}" '  _ = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size)' "${T}:3:"
expect_violation test-birth "${T}" '  _ = CodexRecoveryProcessIdentity.birth(for: getpid())' "${T}:3:"
expect_violation test-windows "${T}" '  _ = CGWindowListCopyWindowInfo([], kCGNullWindowID)' "${T}:3:"
expect_violation test-live-windows "${T}" '  _ = AppDelegate.liveDesktopWindows()' "${T}:3:"
expect_violation test-launch "${T}" '  delegate.applicationDidFinishLaunching(note)' "${T}:3:"
expect_violation test-launchctl "${T}" '  _ = client.stopBackgroundAutomation()' "${T}:3:"
expect_violation test-quit "${T}" '  delegate.quitApp()' "${T}:3:"
expect_violation test-local-bin "${T}" '  let cli = "/Users/me/.local/bin/codex-mon"' "${T}:3:"
expect_violation test-diagnostic-run tests/support/Helpers.swift \
    '  let script = "scripts/swift_live_diagnostics.sh"' 'tests/support/Helpers.swift:1:'
expect_violation suite-runs-diagnostic scripts/test_swift.sh 'bash scripts/swift_live_diagnostics.sh --allow-live-system' \
    'scripts/test_swift.sh:2:'
expect_violation ci-runs-diagnostic .github/workflows/ci.yml '        run: bash scripts/swift_live_diagnostics.sh' \
    '.github/workflows/ci.yml:2:'

# --- The repository follows the rules -----------------------------------------
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests can reach the live CLI, process list, or window list:
${output}"

echo "swift test live system isolation: ok"
