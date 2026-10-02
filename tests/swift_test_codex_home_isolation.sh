#!/bin/bash
# Static check: Swift tests never read, watch, lock, or create the live Codex home.
#
# The Monitor reads its status cache, account registry, Desktop session marker, and auth
# file, and takes its single-instance lock, in the Codex home of the CodexClient it was
# built with. CodexClient.shared resolves the live one (CODEX_HOME, or ~/.codex), which
# ChatGPT.app and the Rust core own. The Swift tests used to build menus with the shared
# client, so they read the developer's registry, and every AppDelegate created ~/.codex
# when it was missing. Tests now build clients on a tests/TestCodexHome.swift directory
# of their own. See
# docs/leanings/2026-09-28-swift-tests-read-the-live-codex-home.md.
#
# Rules, for every *.swift file under tests/ and Sources/ (lines that start with // are
# ignored; block comments, trailing comments, and strings are not):
#   tests   - no CodexClient.shared or CodexClient.liveCodexHome, except in the one
#             wiring check that proves the shared client is on the live home, which must
#             be present; every CodexClient is built with `codexHome:` on the same line;
#             no setenv, unsetenv, or putenv, and no read of CODEX_HOME or HOME from the
#             environment; no API or literal that locates the user's real home
#             (homeDirectoryForCurrentUser, NSHomeDirectory, NSUserName, getpwuid,
#             tilde expansion, "~/", a /Users/.../.codex path).
#   Sources - the live home is resolved once: the "CODEX_HOME" name and a ".codex" path
#             literal each appear once, in CodexClient.swift; liveCodexHome is declared
#             there and used once, by CodexClient(); CodexClient() is built only as
#             CodexClient.shared; only AppDelegate() passes the shared client; only
#             AppDelegate builds the single-instance lock, from its client's home; no
#             source reads HOME from the environment or changes the environment.
# scripts/*.swift helpers are not scanned. test_swift.sh compiles some of them (the
# CodexWindow* helpers and CodexPreservedClipboard) into test binaries, but none of those
# resolves a Codex home; codex-ui-resume.swift, which reads CODEX_HOME, is not compiled
# into any test.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-home-lint.XXXXXX")"
trap '/bin/chmod -R u+rw -- "${TEMP_ROOT}" 2>/dev/null || true; /bin/rm -rf -- "${TEMP_ROOT}"' EXIT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

S='[[:space:]]*'
INIT="(\\.${S}init${S})?"
SHARED_CLIENT="CodexClient${S}\\.${S}shared([^[:alnum:]_]|\$)"
LIVE_HOME="liveCodexHome"
# `.shared` with no type name (e.g. `client: .shared`); other types spell `Type.shared`.
SHARED_SHORTHAND="(^|[^[:alnum:]_.])\\.${S}shared([^[:alnum:]_]|\$)"
# In Sources: a `.shared` default for a CodexClient, and Self.shared / bare shared. in CodexClient.swift.
SOURCE_SHARED_DEFAULT="CodexClient${S}=${S}\\.${S}shared([^[:alnum:]_]|\$)"
CLIENT_SELF_SHARED="Self${S}\\.${S}shared([^[:alnum:]_]|\$)|(^|[^[:alnum:]_.])shared${S}\\."
CLIENT_BUILT="CodexClient${S}${INIT}\\(|CodexClient${S}[?!]?${S}=${S}\\.init${S}\\("
DEFAULT_CLIENT="CodexClient${S}${INIT}\\(${S}\\)|CodexClient${S}[?!]?${S}=${S}\\.init${S}\\(${S}\\)"
ENV_CHANGE="(^|[^[:alnum:]_])(setenv|unsetenv|putenv)${S}\\("
HOME_ENV_READ="(environment${S}\\[|getenv${S}\\()${S}\"(CODEX_)?HOME\""
SOURCE_HOME_ENV_READ="(environment${S}\\[|getenv${S}\\()${S}\"HOME\""
# A fake path under /Users in a fixture is fine; one into a .codex directory there is not.
REAL_HOME="URL${S}\\.${S}homeDirectory|CFFIXED_USER_HOME|homeDirectoryForCurrentUser|homeDirectory${S}\\(${S}forUser|NSHomeDirectory|NSUserName|getpwuid|getpwnam|TildeInPath|CFCopyHomeDirectoryURL|\"~/|/Users/[^\"[:space:]]*/\\.codex"
# The production wiring check, spelled exactly; nothing else may follow it.
WIRING="${S}assertTrue\\(CodexClient\\.shared\\.codexHome == CodexClient\\.liveCodexHome, \"[^\"]*\"\\)\$"
CODEX_HOME_NAME="\"CODEX_HOME\""
CODEX_DIR_LITERAL="[\"/]\\.codex[\"/]"
LIVE_HOME_DECLARED="static var liveCodexHome"
SHARED_ROOT="static let shared = CodexClient\\(\\)"
APP_DELEGATE_ROOT="client:${S}CodexClient\\.shared([^[:alnum:]_.]|\$)"
LOCK_BUILT="SingleInstanceGuard${S}${INIT}\\("
LOCK_ROOT="SingleInstanceGuard\\(codexHome:${S}client\\.codexHome\\)"

# code_matches FILE ERE -> "file:line:text" for each matching line that is not a //
# comment. A file grep cannot read is reported as a match, so it fails.
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

# expect_once FILE ERE WHAT -> adds a violation unless exactly one code line of the
# Sources tree matches ERE and it is in FILE
expect_once() {
    local found count
    found="$(for file in "${SOURCES[@]}"; do code_matches "${file}" "$2"; done)"
    count="$(count_matching "${found}" '.')"
    if [ "${count}" -ne 1 ] || [ "$(count_matching "${found}" "^$1:")" -ne 1 ]; then
        violations+="$3 must appear once in Sources, in $1; found ${count}"
        [ -z "${found}" ] || violations+=":"$'\n'"${found}"
        violations+=$'\n'
    fi
}

# check_tree ROOT -> prints each violation; returns 1 if there is any
check_tree() {
    ROOT="$1"
    local violations="" file found wiring=0 client_file=Sources/CodexClient.swift
    local delegate_file=Sources/AppDelegate.swift
    while IFS= read -r file; do
        [ -z "$(code_matches "${file}" "^${WIRING}")" ] || wiring=1
        found="$(without "$(code_matches "${file}" "${SHARED_CLIENT}|${LIVE_HOME}|${SHARED_SHORTHAND}")" "${WIRING}")"
        [ -z "${found}" ] \
            || violations+="${found} (build a CodexClient on a TestCodexHome; only the wiring check names the live home)"$'\n'
        found="$(without "$(code_matches "${file}" "${CLIENT_BUILT}")" '.*codexHome:')"
        [ -z "${found}" ] || violations+="${found} (build CodexClient( with codexHome: on the same line)"$'\n'
        found="$(code_matches "${file}" "${ENV_CHANGE}|${HOME_ENV_READ}")"
        [ -z "${found}" ] \
            || violations+="${found} (pass a TestCodexHome instead of reading or changing the environment)"$'\n'
        found="$(code_matches "${file}" "${REAL_HOME}")"
        [ -z "${found}" ] || violations+="${found} (tests must not locate the user's real home)"$'\n'
    done < <(/usr/bin/find "${ROOT}/tests" -type f -name '*.swift' | /usr/bin/sort)
    [ "${wiring}" -eq 1 ] \
        || violations+="tests: no check that CodexClient.shared uses the live Codex home"$'\n'

    SOURCES=()
    while IFS= read -r file; do
        SOURCES+=("${file}")
    done < <(/usr/bin/find "${ROOT}/Sources" -type f -name '*.swift' | /usr/bin/sort)
    for file in "${client_file}" "${delegate_file}"; do
        [ -f "${ROOT}/${file}" ] || violations+="${file} is missing; it holds a composition root"$'\n'
    done
    expect_once "${client_file}" "${CODEX_HOME_NAME}" 'The "CODEX_HOME" name'
    expect_once "${client_file}" "${CODEX_DIR_LITERAL}" 'A ".codex" path literal'
    expect_once "${client_file}" "${LIVE_HOME_DECLARED}" 'The liveCodexHome declaration'
    expect_once "${client_file}" "${DEFAULT_CLIENT}" 'CodexClient()'
    expect_once "${client_file}" "${SHARED_ROOT}" 'The shared client'
    found="$(code_matches "${ROOT}/${client_file}" "${CLIENT_SELF_SHARED}" 2>/dev/null)"
    [ -z "${found}" ] \
        || violations+="${found} (CodexClient must not route through its own shared instance)"$'\n'
    expect_once "${delegate_file}" "${SHARED_CLIENT}" 'CodexClient.shared'
    expect_once "${delegate_file}" "${APP_DELEGATE_ROOT}" 'The AppDelegate() client argument'
    expect_once "${delegate_file}" "${LOCK_BUILT}" 'The single-instance lock'
    expect_once "${delegate_file}" "${LOCK_ROOT}" 'The lock in the client home'
    # liveCodexHome: its declaration and the one use in CodexClient().
    found="$(for file in "${SOURCES[@]}"; do code_matches "${file}" "${LIVE_HOME}"; done)"
    if [ "$(count_matching "${found}" '.')" -ne 2 ] \
        || [ "$(count_matching "${found}" "^${client_file}:")" -ne 2 ]; then
        violations+="liveCodexHome must be declared and used once, both in ${client_file}"
        [ -z "${found}" ] || violations+=":"$'\n'"${found}"
        violations+=$'\n'
    fi
    for file in "${SOURCES[@]}"; do
        found="$(code_matches "${file}" "${SOURCE_SHARED_DEFAULT}")"
        [ -z "${found}" ] \
            || violations+="${found} (do not default a CodexClient to .shared; only AppDelegate() passes it)"$'\n'
        found="$(code_matches "${file}" "${ENV_CHANGE}|${SOURCE_HOME_ENV_READ}")"
        [ -z "${found}" ] \
            || violations+="${found} (sources must not change the environment or read HOME from it)"$'\n'
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
        '// Never use CodexClient.shared here.' \
        '  let client = CodexClient(codexHome: home.url, distributionRunner: { _ in false })' \
        '  let delegate = AppDelegate(client: client, defaults: suite, autoLaunchManager: manager)' \
        '  let resolved = CodexClient.resolveCodexHome(environment: ["CODEX_HOME": ""], userHome: user)' \
        '  let listing = "{\"/Users/me/Applications/Other App.app/\"}"' \
        > "${dir}/tests/AppDelegateTests.swift"
    printf '%s\n' \
        '  assertTrue(CodexClient.shared.codexHome == CodexClient.liveCodexHome, "shared client on the live home")' \
        > "${dir}/tests/AppDelegateWiringTests.swift"
    printf '%s\n' \
        '  public static let shared = CodexClient()' \
        '    let home = Self.liveCodexHome' \
        '  internal static var liveCodexHome: URL {' \
        '      userHome: FileManager.default.homeDirectoryForCurrentUser)' \
        '    if let configured = environment["CODEX_HOME"], !configured.isEmpty {' \
        '    return userHome.appendingPathComponent(".codex", isDirectory: true)' \
        > "${dir}/Sources/CodexClient.swift"
    printf '%s\n' \
        '      client: CodexClient.shared, defaults: UserDefaults.standard,' \
        '    self.singleGuard = SingleInstanceGuard(codexHome: client.codexHome)' \
        > "${dir}/Sources/AppDelegate.swift"
    printf '%s\n' '    let statusPath = client.statusFileURL.path' > "${dir}/Sources/AppDelegate+FileWatchers.swift"
    echo "${dir}"
}
expect_violation() {
    # expect_violation NAME FILE LINE PATTERN: the fixture with LINE appended to FILE is rejected
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
expect_violation test-shared-client "${T}" \
    'let delegate = makeTestAppDelegate(client: CodexClient.shared, preferences: preferences)' "${T}:6:"
expect_violation test-shared-client-read "${T}" \
    'let enabled = CodexClient.shared.getAutoSwitchSettings()' 'only the wiring check names the live home'
expect_violation test-live-home "${T}" 'let home = CodexClient.liveCodexHome' "${T}:6:"
expect_violation test-wiring-line-with-read "${T}" \
    '  assertTrue(CodexClient.shared.codexHome == CodexClient.liveCodexHome, "x"); _ = CodexClient.shared.loadCachedSnapshot()' "${T}:6:"
expect_violation test-default-client "${T}" 'let client = CodexClient()' 'codexHome: on the same line'
expect_violation test-default-client-init "${T}" 'let client = CodexClient.init()' 'codexHome: on the same line'
expect_violation test-default-client-shorthand "${T}" 'let client: CodexClient = .init()' 'codexHome: on the same line'
expect_violation test-client-without-home "${T}" \
    'let client = CodexClient(distributionRunner: { _ in true })' 'codexHome: on the same line'
expect_violation test-client-home-on-next-line "${T}" 'let client = CodexClient(' 'codexHome: on the same line'
expect_violation test-setenv "${T}" 'setenv("CODEX_HOME", home.path, 1)' 'reading or changing the environment'
expect_violation test-unsetenv "${T}" 'unsetenv("CODEX_HOME")' 'reading or changing the environment'
expect_violation test-putenv "${T}" 'putenv(strdup("HOME=/tmp"))' 'reading or changing the environment'
expect_violation test-getenv "${T}" 'let previous = getenv("CODEX_HOME")' 'reading or changing the environment'
expect_violation test-environment-read "${T}" \
    'let previous = ProcessInfo.processInfo.environment["HOME"]' 'reading or changing the environment'
expect_violation test-real-home "${T}" \
    'let registry = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex/accounts.json")' \
    "tests must not locate the user's real home"
expect_violation test-ns-home "${T}" 'let home = NSHomeDirectory()' "tests must not locate the user's real home"
expect_violation test-tilde "${T}" 'let home = ("~/.codex" as NSString).expandingTildeInPath' \
    "tests must not locate the user's real home"
expect_violation test-users-path "${T}" 'let home = URL(fileURLWithPath: "/Users/someone/.codex")' \
    "tests must not locate the user's real home"
expect_violation test-users-subpath "${T}" 'let registry = "/Users/someone/.codex/accounts.json"' \
    "tests must not locate the user's real home"
expect_violation test-shared-shorthand-arg "${T}" \
    'let delegate = makeTestAppDelegate(client: .shared, preferences: preferences)' 'only the wiring check names the live home'
expect_violation test-shared-shorthand-typed "${T}" 'let client: CodexClient = .shared' 'only the wiring check names the live home'
expect_violation test-url-home-directory "${T}" 'let home = URL.homeDirectory.appending(path: ".codex")' \
    "tests must not locate the user's real home"
expect_violation test-resolve-real-home "${T}" \
    'let h = CodexClient.resolveCodexHome(environment: [:], userHome: URL.homeDirectory)' "tests must not locate the user's real home"
expect_violation test-cffixed-home "${T}" 'let fixed = "CFFIXED_USER_HOME"' "tests must not locate the user's real home"
expect_violation test-in-subdirectory tests/support/Helpers.swift \
    'let client = CodexClient.shared' 'tests/support/Helpers.swift:1:'
expect_missing test-no-wiring-check tests/AppDelegateWiringTests.swift \
    'no check that CodexClient.shared uses the live Codex home'

C=Sources/CodexClient.swift
D=Sources/AppDelegate.swift
W=Sources/AppDelegate+FileWatchers.swift
expect_violation source-shared-default "${W}" \
    'func make(client: CodexClient = .shared) {}' 'do not default a CodexClient to .shared'
expect_violation source-self-shared "${C}" \
    '  public static var statusFileURL: URL { Self.shared.statusFileURL }' 'must not route through its own shared instance'
expect_violation source-bare-shared "${C}" \
    '  static var statusPath: String { shared.statusFileURL.path }' 'must not route through its own shared instance'
expect_violation source-static-home "${C}" \
    '  public static var statusFileURL: URL { liveCodexHome.appendingPathComponent("usage-status.json") }' \
    'liveCodexHome must be declared and used once'
expect_violation source-home-elsewhere "${W}" '    let statusPath = CodexClient.liveCodexHome.path' \
    'liveCodexHome must be declared and used once'
expect_violation source-second-codex-dir "${W}" \
    '    let home = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex")' \
    'A ".codex" path literal must appear once'
expect_violation source-codex-subpath Sources/QuotaModels.swift \
    '    let codexHelpURL = home.appendingPathComponent(".codex/helps.html")' 'A ".codex" path literal must appear once'
expect_violation source-second-env-name "${W}" \
    '    let home = ProcessInfo.processInfo.environment["CODEX_HOME"] ?? ""' 'The "CODEX_HOME" name must appear once'
expect_violation source-second-default-client "${W}" '    let client = CodexClient()' 'CodexClient() must appear once'
expect_violation source-second-shared-client "${W}" '    let snapshot = CodexClient.shared.loadCachedSnapshot()' \
    'CodexClient.shared must appear once'
expect_violation source-home-env Sources/SingleInstanceGuard.swift \
    '      (ProcessInfo.processInfo.environment["HOME"]).flatMap { $0.isEmpty ? nil : $0 }' \
    'read HOME from it'
expect_violation source-setenv "${W}" '    setenv("CODEX_HOME", path, 1)' 'change the environment'
expect_violation source-second-lock Sources/AppDelegate+WindowBounds.swift \
    '    let other = SingleInstanceGuard(codexHome: client.codexHome)' 'The single-instance lock must appear once'
expect_replaced source-lock-elsewhere "${D}" \
    "$(printf '%s\n' '      client: CodexClient.shared, defaults: UserDefaults.standard,' \
        '    self.singleGuard = SingleInstanceGuard(codexHome: URL(fileURLWithPath: "/tmp/codex"))')" \
    'The lock in the client home must appear once'
expect_replaced source-lost-root "${D}" \
    "$(printf '%s\n' '      client: CodexClient(), defaults: UserDefaults.standard,' \
        '    self.singleGuard = SingleInstanceGuard(codexHome: client.codexHome)')" \
    'The AppDelegate() client argument must appear once'
expect_replaced source-lost-live-home "${C}" \
    "$(printf '%s\n' '  public static let shared = CodexClient()' \
        '    let home = URL(fileURLWithPath: NSTemporaryDirectory())' \
        '    if let configured = environment["CODEX_HOME"], !configured.isEmpty {' \
        '    return userHome.appendingPathComponent(".codex", isDirectory: true)')" \
    'The liveCodexHome declaration must appear once'
expect_missing source-missing-root "${C}" 'Sources/CodexClient.swift is missing'

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
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests or sources can reach the live Codex home:
${output}"

echo "swift test codex home isolation: ok"
