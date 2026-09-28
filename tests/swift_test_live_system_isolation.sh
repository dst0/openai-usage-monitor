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
#             without a call; the body of CodexClient() is exactly
#             CLIENT_ROOT_BODY, so it cannot call what it binds; CodexClient has
#             no static cliExecutableURL; a
#             ~/.local/bin path appears only in the installed-CLI lookup and in
#             refreshCLIVersion (which only the app's launch runs); the window
#             list is read only in liveDesktopWindows among the AppDelegate files.
#   tests   - no name of a live lookup or of a path that reaches one: the three
#             above, NSRunningApplication, NSWorkspace, runningApplications,
#             proc_pidinfo, sysctl, KERN_PROC, /bin/ps, pgrep,
#             CodexRecoveryProcessIdentity.birth, any CGWindowList function,
#             isAnotherInstanceRunning, refreshCLIVersion,
#             applicationDidFinishLaunching, start/stopBackgroundAutomation,
#             quitApp (both run launchctl), a .local/bin path, or the
#             diagnostic script.
#   The diagnostic reads the live system only with --allow-live-system. No
#   Swift test, shell test (other than this check, whose fixtures hold it),
#   scripts/test_swift.sh, or workflow passes that flag; the workflows never
#   name the diagnostic, and scripts/test_swift.sh names it only to build it
#   with --compile-only, so its sources cannot go stale. Running it with any
#   other arguments stops before the build, and --compile-only never runs what
#   it built; the check proves both with a fake swiftc.
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
DIAGNOSTIC_BUILD="bash scripts/swift_live_diagnostics\\.sh --compile-only\$"
LIVE_FLAG="--allow-live-system"
TEST_FORBIDDEN="${CLI_LOOKUP}|${DESKTOP_LOOKUP}|${WINDOWS_LOOKUP}|NSRunningApplication|NSWorkspace|runningApplications|proc_pidinfo|sysctl|KERN_PROC|/bin/ps|pgrep|CodexRecoveryProcessIdentity${S}\\.${S}birth|CGWindowList|isAnotherInstanceRunning|refreshCLIVersion|applicationDidFinishLaunching|startBackgroundAutomation|stopBackgroundAutomation|quitApp|${LOCAL_BIN}|${DIAGNOSTIC}"
# The whole body of CodexClient(): it binds the live lookups and calls none of them.
CLIENT_ROOT_BODY="$(printf '%s\n' \
    '  public convenience init() {' \
    '    let home = Self.liveCodexHome' \
    '    let cli = Self.installedCLIExecutable' \
    '    let desktop = CodexDesktopProcessIdentity.current' \
    '    self.init(' \
    '      codexHome: home,' \
    '      distributionRunner: { Self.runDistributionProcess(executable: cli(), arguments: $0) },' \
    '      cliExecutable: cli, desktopProcess: desktop,' \
    '      desktopAppAccountIdProvider: { Self.readDesktopAppSessionAccountId(in: home, currentProcess: desktop) })' \
    '  }')"

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
    done < <(printf '%s\n' "${matches}")
}

# with_reason LINES REASON -> each line of LINES followed by " (REASON)"
with_reason() {
    [ -z "$1" ] || printf '%s\n' "$1" | /usr/bin/awk -v reason="$2" '{ print $0 " (" reason ")" }'
}

# check_tree ROOT -> prints each violation; returns 1 if there is any
check_tree() {
    ROOT="$1"
    local violations="" file rel found all body
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
        violations+="$(with_reason "$(code_matches "${file}" "${STATIC_CLI_URL}")" \
            'CodexClient takes its CLI executable; no static lookup')"$'\n'
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
    body="$(/usr/bin/awk '/^  public convenience init[(][)] [{]$/ { p = 1 } p { print } p && /^  [}]$/ { exit }' \
        "${ROOT}/Sources/CodexClient.swift" 2>/dev/null)" || body=""
    [ "${body}" = "${CLIENT_ROOT_BODY}" ] \
        || violations+="Sources/CodexClient.swift: the body of CodexClient() must be exactly CLIENT_ROOT_BODY in this check, binding the live lookups without calling them"$'\n'

    all="$(/usr/bin/find "${ROOT}/tests" -type f -name '*.swift')" || { printf 'cannot list tests\n'; return 1; }
    while IFS= read -r file; do
        [ -n "${file}" ] || continue
        violations+="$(with_reason "$(code_matches "${file}" "${TEST_FORBIDDEN}")" \
            'tests must not reach the live CLI, process list, window list, or launchctl')"$'\n'
    done < <(printf '%s\n' "${all}" | /usr/bin/sort)
    all="$(/usr/bin/find "${ROOT}/tests" -type f -name '*.sh')" || { printf 'cannot list tests\n'; return 1; }
    all+=$'\n'"$(/usr/bin/find "${ROOT}/.github/workflows" -type f \( -name '*.yml' -o -name '*.yaml' \) 2>/dev/null || true)"
    all+=$'\n'"${ROOT}/scripts/test_swift.sh"
    while IFS= read -r file; do
        [ -n "${file}" ] || continue
        rel="${file#"${ROOT}"/}"
        [ "${rel}" != tests/swift_test_live_system_isolation.sh ] || continue
        [ -f "${file}" ] || continue
        violations+="$(with_reason "$(code_matches "${file}" "${LIVE_FLAG}")" \
            'only a person or agent that needs live evidence passes this flag')"$'\n'
        case "${rel}" in
            .github/workflows/*) violations+="$(with_reason "$(code_matches "${file}" "${DIAGNOSTIC}")" \
                'no workflow runs the live diagnostic')"$'\n' ;;
        esac
    done < <(printf '%s\n' "${all}" | /usr/bin/sort)
    violations+="$(expect_only 'the live diagnostic' \
        "$(code_matches "${ROOT}/scripts/test_swift.sh" "${DIAGNOSTIC}")" \
        "scripts/test_swift.sh:[0-9]+:${DIAGNOSTIC_BUILD}")"$'\n'
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
        "${CLIENT_ROOT_BODY}" \
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
    printf '%s\n' 'swiftc -parse-as-library tests/AppDelegateTests.swift' \
        'bash scripts/swift_live_diagnostics.sh --compile-only' > "${dir}/scripts/test_swift.sh"
    printf '%s\n' 'bash tests/swift_module_cache_path.sh' > "${dir}/tests/other_test.sh"
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
expect_edited() {
    # expect_edited NAME FILE SED PATTERN: the fixture with SED applied to FILE is rejected with PATTERN
    local dir output
    dir="$(make_fixture "$1")"
    /usr/bin/sed -e "$3" "${dir}/$2" > "${dir}/$2.new" && /bin/mv "${dir}/$2.new" "${dir}/$2"
    if output="$(check_tree "${dir}")"; then
        fail "$1: the checker accepted $2 edited with: $3"
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
# The root binds its lookups and calls none of them, not even through its bindings.
expect_edited source-root-calls-binding "${C}" '/let desktop = /a\
    _ = desktop()' 'the body of CodexClient() must be exactly CLIENT_ROOT_BODY'
expect_edited source-root-missing "${C}" 's/public convenience init() {/public init() {/' \
    'the body of CodexClient() must be exactly CLIENT_ROOT_BODY'

T=tests/AppDelegateTests.swift
expect_violation test-cli "${T}" '  let url = CodexClient.installedCLIExecutable()' "${T}:3:"
expect_violation test-desktop "${T}" '  let running = CodexDesktopProcessIdentity .current() != nil' "${T}:3:"
expect_violation test-apps "${T}" '  let apps = NSWorkspace.shared.runningApplications' "${T}:3:"
expect_violation test-proc "${T}" '  _ = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size)' "${T}:3:"
expect_violation test-workspace "${T}" '  let app = NSWorkspace.shared.frontmostApplication' "${T}:3:"
expect_violation test-sysctl "${T}" '  _ = sysctl(&mib, 4, &info, &size, nil, 0)' "${T}:3:"
expect_violation test-ps "${T}" '  process.executableURL = URL(fileURLWithPath: "/bin/ps")' "${T}:3:"
expect_violation test-pgrep "${T}" '  let args = ["/usr/bin/pgrep", "-x", "ChatGPT"]' "${T}:3:"
expect_violation test-window-image "${T}" '  _ = CGWindowListCreateImage(.null, .optionAll, 0, [])' "${T}:3:"
expect_violation test-birth "${T}" '  _ = CodexRecoveryProcessIdentity.birth(for: getpid())' "${T}:3:"
expect_violation test-windows "${T}" '  _ = CGWindowListCopyWindowInfo([], kCGNullWindowID)' "${T}:3:"
expect_violation test-live-windows "${T}" '  _ = AppDelegate.liveDesktopWindows()' "${T}:3:"
expect_violation test-launch "${T}" '  delegate.applicationDidFinishLaunching(note)' "${T}:3:"
expect_violation test-launchctl "${T}" '  _ = client.stopBackgroundAutomation()' "${T}:3:"
expect_violation test-quit "${T}" '  delegate.quitApp()' "${T}:3:"
expect_violation test-local-bin "${T}" '  let cli = "/Users/me/.local/bin/codex-mon"' "${T}:3:"
# Every violating line carries its reason.
expect_violation test-two-lines "${T}" '  _ = proc_pidinfo(pid, 0, 0, nil, 0)'$'\n''  _ = proc_pidinfo(pid, 1, 0, nil, 0)' \
    "${T}:3:  _ = proc_pidinfo(pid, 0, 0, nil, 0) (tests must not reach"
expect_violation test-two-lines-last "${T}" '  _ = proc_pidinfo(pid, 0, 0, nil, 0)'$'\n''  _ = proc_pidinfo(pid, 1, 0, nil, 0)' \
    "${T}:4:  _ = proc_pidinfo(pid, 1, 0, nil, 0) (tests must not reach"
expect_violation test-diagnostic-run tests/support/Helpers.swift \
    '  let script = "scripts/swift_live_diagnostics.sh"' 'tests/support/Helpers.swift:1:'
expect_violation suite-runs-diagnostic scripts/test_swift.sh 'bash scripts/swift_live_diagnostics.sh --allow-live-system' \
    'scripts/test_swift.sh:3:bash scripts/swift_live_diagnostics.sh --allow-live-system (only a person or agent'
expect_replaced suite-skips-diagnostic-build scripts/test_swift.sh 'swiftc -parse-as-library tests/AppDelegateTests.swift' \
    'the live diagnostic must be named exactly once'
expect_violation shell-test-runs-diagnostic tests/other_test.sh 'bash scripts/swift_live_diagnostics.sh --allow-live-system' \
    'tests/other_test.sh:2:'
expect_violation ci-runs-diagnostic .github/workflows/ci.yml '        run: bash scripts/swift_live_diagnostics.sh' \
    '.github/workflows/ci.yml:2:        run: bash scripts/swift_live_diagnostics.sh (no workflow runs'
expect_violation yaml-runs-diagnostic .github/workflows/live.yaml '        run: bash scripts/swift_live_diagnostics.sh --allow-live-system' \
    '.github/workflows/live.yaml:1:'

# --- The diagnostic builds only when asked and runs only with its flag ----------
# A fake swiftc, first on PATH, records each build and writes a program that
# records each run. The live mode is never run here, so even a fake that failed
# to take effect could not read the live system: the other modes stop before
# the build or never run what they built.
DIAGNOSTIC_SCRIPT="${PROJECT_DIR}/scripts/swift_live_diagnostics.sh"
if /usr/bin/grep -n -E 'xcrun|/swiftc|SWIFTC|\$\{?[A-Za-z_]*\}?/swiftc' "${DIAGNOSTIC_SCRIPT}"; then
    fail "scripts/swift_live_diagnostics.sh must run swiftc from PATH, so this check's fake replaces it"
fi
fake_bin="${TEMP_ROOT}/fake-bin"
build_log="${TEMP_ROOT}/diagnostic.log"
/bin/mkdir -p "${fake_bin}" "${TEMP_ROOT}/diagnostic-tmp"
printf '%s\n' '#!/bin/bash' \
    "printf 'built\\n' >> '${build_log}'" \
    'out=""; while [ "$#" -gt 0 ]; do [ "$1" != -o ] || out="$2"; shift; done' \
    "printf '#!/bin/bash\\nprintf \"ran\\\\n\" >> \"%s\"\\n' '${build_log}' > \"\${out}\"" \
    '/bin/chmod 700 "${out}"' > "${fake_bin}/swiftc"
/bin/chmod 700 "${fake_bin}/swiftc"
run_diagnostic() {
    # run_diagnostic EXPECTED_STATUS EXPECTED_LOG ARGS...: runs the diagnostic with the fake swiftc
    local expected_status="$1" expected_log="$2" status=0 log
    shift 2
    : > "${build_log}"
    /usr/bin/env -u CLANG_MODULE_CACHE_PATH PATH="${fake_bin}:/usr/bin:/bin" TMPDIR="${TEMP_ROOT}/diagnostic-tmp" \
        /bin/bash "${DIAGNOSTIC_SCRIPT}" "$@" > /dev/null 2>&1 || status=$?
    log="$(/bin/cat "${build_log}")"
    [ "${status}" -eq "${expected_status}" ] && [ "${log}" = "${expected_log}" ] \
        || fail "the diagnostic with [$*] exited ${status} and recorded [${log//$'\n'/ }]; expected ${expected_status} and [${expected_log//$'\n'/ }]"
}
run_diagnostic 2 '' 
run_diagnostic 2 '' --allow-live
run_diagnostic 2 '' --compile-only --allow-live-system
run_diagnostic 2 '' --allow-live-system --compile-only
run_diagnostic 0 'built' --compile-only
[ -z "$(/usr/bin/find "${TEMP_ROOT}/diagnostic-tmp" -mindepth 1)" ] || fail "the diagnostic left its build directory behind"
# The fake itself works: its program records a run when run.
/usr/bin/env PATH="${fake_bin}:/usr/bin:/bin" swiftc -o "${TEMP_ROOT}/probe" > /dev/null
: > "${build_log}"
"${TEMP_ROOT}/probe"
[ "$(/bin/cat "${build_log}")" = ran ] || fail "the fake swiftc's program does not record its run"

# --- The repository follows the rules -----------------------------------------
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests can reach the live CLI, process list, or window list:
${output}"

echo "swift test live system isolation: ok"
