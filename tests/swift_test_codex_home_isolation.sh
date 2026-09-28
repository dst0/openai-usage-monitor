#!/bin/bash
# Static check: every Swift test build defines CODEX_MONITOR_TESTS.
#
# That define compiles the Codex home tripwires into Sources/ (CodexClient.codexHome,
# SingleInstanceGuard.defaultLockPath, and the help page's home fallback), which
# tests/TestCodexHome.swift backs: a test that would resolve the live ~/.codex
# stops before the path is built. A test build without the define has no
# tripwires, so its tests could read the live ~/.codex unnoticed. With it, a
# build that compiles a tripwired Source without tests/TestCodexHome.swift does
# not compile, and tests/TestCodexHome.swift does not compile without it.
#
# Two rules, each proved on fixtures below before the repository is checked:
#   scripts/test_swift.sh compiles only through one wrapper that adds the define.
#     The wrapper must be spelled exactly and defined once; any other line there
#     that names swiftc or defines swiftc_test fails. Lines whose first non-blank
#     character is # are ignored.
#   Sources/*.swift names the live Codex home (a ".codex" or "/.codex" string, or
#     the "CODEX_HOME" variable) only on the lines listed in LIVE_HOME_SITES, each
#     in a file with a tripwire. A new one needs a tripwire before it, a child
#     probe in tests/TestCodexHomeTests.swift, and an entry in that list. Lines
#     whose first non-blank characters are // are ignored.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-test-define-lint.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT

WRAPPER='swiftc_test() { swiftc -D CODEX_MONITOR_TESTS "$@"; }'
# The swiftc command word, and any definition of the wrapper's name.
SWIFTC_WORD='(^|[^[:alnum:]_])swiftc([^[:alnum:]_]|$)'
WRAPPER_DEFINITION='(^|[^[:alnum:]_])swiftc_test[[:space:]]*\(|function[[:space:]]+swiftc_test([^[:alnum:]_]|$)'

LIVE_HOME_NAMES='"\.codex|/\.codex|"CODEX_HOME"'
TRIPWIRE='TestCodexHome\.(requireIsolated|forbid)\('
# "<file> <lines>": each Sources file that names the live Codex home, and on how
# many lines. Every other Sources file names it on none.
LIVE_HOME_SITES=(
    "CodexClient.swift 2"
    "QuotaModels.swift 1"
    "SingleInstanceGuard.swift 1"
)

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# check_script FILE -> prints each violation; returns 1 if there is any
check_script() {
    local file="$1" name="${1##*/}" violations="" line number=0 wrappers=0
    if [ ! -f "$1" ] || [ ! -r "$1" ]; then
        printf '%s: cannot be read\n' "${name}"
        return 1
    fi
    while IFS= read -r line || [ -n "${line}" ]; do
        number=$((number + 1))
        [[ "${line}" =~ ^[[:space:]]*# ]] && continue
        [[ "${line}" =~ ${SWIFTC_WORD} || "${line}" =~ ${WRAPPER_DEFINITION} ]] || continue
        if [ "${line}" = "${WRAPPER}" ]; then
            wrappers=$((wrappers + 1))
        else
            violations+="${name}:${number}: ${line} (compile only through swiftc_test)"$'\n'
        fi
    done < "$1"
    [ "${wrappers}" -eq 1 ] \
        || violations+="${name} defines the wrapper ${wrappers} times; define it once as: ${WRAPPER}"$'\n'
    if [ -n "${violations}" ]; then
        printf '%s' "${violations}"
        return 1
    fi
}

# check_sources DIR -> prints each violation; returns 1 if there is any
check_sources() {
    local dir="$1" violations="" file name expected site count
    [ -d "${dir}" ] || { printf '%s: no Sources directory\n' "${dir}"; return 1; }
    for site in "${LIVE_HOME_SITES[@]}"; do
        [ -f "${dir}/${site% *}" ] || violations+="Sources/${site% *} is missing; LIVE_HOME_SITES lists it"$'\n'
    done
    while IFS= read -r file; do
        name="${file##*/}"
        expected=0
        for site in "${LIVE_HOME_SITES[@]}"; do
            [ "${site% *}" = "${name}" ] && expected="${site##* }"
        done
        count="$(/usr/bin/grep -E -v '^[[:space:]]*//' "${file}" | /usr/bin/grep -c -E -- "${LIVE_HOME_NAMES}" || true)"
        if [ "${count}" -ne "${expected}" ]; then
            violations+="Sources/${name} names the live Codex home on ${count} lines, not ${expected} (a new one needs a tripwire, a probe, and an entry in LIVE_HOME_SITES)"$'\n'
        fi
        if [ "${expected}" -gt 0 ] && ! /usr/bin/grep -q -E -- "${TRIPWIRE}" "${file}"; then
            violations+="Sources/${name} names the live Codex home without a tripwire"$'\n'
        fi
    done < <(/usr/bin/find "${dir}" -type f -name '*.swift' | /usr/bin/sort)
    if [ -n "${violations}" ]; then
        printf '%s' "${violations}"
        return 1
    fi
}

# --- The checker rejects each kind of violation --------------------------------
make_fixture() {
    local dir="${TEMP_ROOT}/$1"
    /bin/mkdir -p "${dir}"
    printf '%s\n' \
        '#!/bin/bash' \
        '# Resolve the module cache before any swiftc call.' \
        "${WRAPPER}" \
        'swiftc_test -parse-as-library \' \
        '    Sources/CodexClient.swift \' \
        '    tests/TestCodexHome.swift \' \
        '    -o "${TMP_BIN_DIR}/client_test"' > "${dir}/test_swift.sh"
    echo "${dir}/test_swift.sh"
}
expect_violation() {
    # expect_violation NAME LINE PATTERN: the fixture plus LINE is rejected with PATTERN
    local script output
    script="$(make_fixture "$1")"
    printf '%s\n' "$2" >> "${script}"
    if output="$(check_script "${script}")"; then
        fail "$1: the checker accepted: $2"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$3" || fail "$1: unexpected report: ${output}"
}
expect_replaced() {
    # expect_replaced NAME CONTENT PATTERN: the fixture replaced by CONTENT is rejected with PATTERN
    local script output
    script="$(make_fixture "$1")"
    printf '%s\n' "$2" > "${script}"
    if output="$(check_script "${script}")"; then
        fail "$1: the checker accepted: $2"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$3" || fail "$1: unexpected report: ${output}"
}

clean="$(make_fixture clean)"
output="$(check_script "${clean}")" || fail "clean fixture was rejected: ${output}"

expect_violation bare 'swiftc -parse-as-library tests/A.swift -o a_test' 'test_swift.sh:8: swiftc -parse-as-library'
expect_violation indented '    swiftc \' 'test_swift.sh:8:     swiftc \'
expect_violation xcrun 'xcrun swiftc -o a_test tests/A.swift' 'test_swift.sh:8: xcrun swiftc'
expect_violation substitution 'SWIFTC="$(xcrun --find swiftc)"' 'test_swift.sh:8: SWIFTC='
expect_violation trailing 'build() { swiftc "$@"; }' 'test_swift.sh:8: build()'
expect_violation undefined-wrapper 'swiftc_test() { swiftc "$@"; }' 'test_swift.sh:8: swiftc_test() { swiftc "$@"; }'
expect_violation redefined-wrapper 'swiftc_test () { "$@"; }' 'test_swift.sh:8: swiftc_test () {'
expect_violation function-wrapper 'function swiftc_test { command "$@"; }' 'test_swift.sh:8: function swiftc_test'
expect_violation duplicate-wrapper "${WRAPPER}" 'defines the wrapper 2 times'
expect_replaced missing-wrapper 'swiftc_test -o a_test tests/A.swift' 'defines the wrapper 0 times'
missing="${TEMP_ROOT}/missing/test_swift.sh"
if output="$(check_script "${missing}")"; then
    fail "missing: the checker accepted a script that does not exist"
fi
printf '%s\n' "${output}" | /usr/bin/grep -qF 'test_swift.sh: cannot be read' \
    || fail "missing: unexpected report: ${output}"

make_sources() {
    local dir="${TEMP_ROOT}/$1/Sources"
    /bin/mkdir -p "${dir}"
    printf '%s\n' \
        '    TestCodexHome.requireIsolated(env, seam: "CodexClient.codexHome")' \
        '    if let env = ProcessInfo.processInfo.environment["CODEX_HOME"], !env.isEmpty {' \
        '    TestCodexHome.forbid("CodexClient.codexHome with CODEX_HOME unset")' \
        '    return home.appendingPathComponent(".codex")' > "${dir}/CodexClient.swift"
    printf '%s\n' \
        '    TestCodexHome.forbid("HelpsDocHelper.findHelpsHTMLURL home fallback")' \
        '    let codexHelpURL = home.appendingPathComponent(".codex/helps.html")' > "${dir}/QuotaModels.swift"
    printf '%s\n' \
        '    TestCodexHome.forbid("SingleInstanceGuard.defaultLockPath")' \
        '    let codexDir = (home as NSString).appendingPathComponent(".codex")' > "${dir}/SingleInstanceGuard.swift"
    printf '%s\n' \
        '  /// Built on first use: its default path is the live `~/.codex/monitor.lock`.' \
        '  internal lazy var singleGuard = SingleInstanceGuard()' > "${dir}/AppDelegate.swift"
    echo "${dir}"
}
expect_sources_violation() {
    # expect_sources_violation NAME FILE LINE PATTERN: the Sources fixture plus LINE in FILE is rejected
    local dir output
    dir="$(make_sources "$1")"
    printf '%s\n' "$3" >> "${dir}/$2"
    if output="$(check_sources "${dir}")"; then
        fail "$1: the checker accepted $2 with: $3"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF -- "$4" || fail "$1: unexpected report: ${output}"
}

clean_sources="$(make_sources clean-sources)"
output="$(check_sources "${clean_sources}")" || fail "clean Sources fixture was rejected: ${output}"

expect_sources_violation new-site AccountRowView.swift \
    'let marker = home.appendingPathComponent(".codex/desktop-app-session.json")' \
    'Sources/AccountRowView.swift names the live Codex home on 1 lines, not 0'
expect_sources_violation new-slash-site AppDelegate.swift \
    'let lock = NSHomeDirectory() + "/.codex/desktop-recovery.lock"' \
    'Sources/AppDelegate.swift names the live Codex home on 1 lines, not 0'
expect_sources_violation second-env-read AppDelegate+FileWatchers.swift \
    'let home = ProcessInfo.processInfo.environment["CODEX_HOME"] ?? "~"' \
    'Sources/AppDelegate+FileWatchers.swift names the live Codex home on 1 lines, not 0'
expect_sources_violation extra-listed-site QuotaModels.swift \
    '    let legacy = home.appendingPathComponent(".codex/help.html")' \
    'Sources/QuotaModels.swift names the live Codex home on 2 lines, not 1'
sources_without_tripwire="$(make_sources no-tripwire)"
printf '%s\n' '    let codexDir = (home as NSString).appendingPathComponent(".codex")' \
    > "${sources_without_tripwire}/SingleInstanceGuard.swift"
if output="$(check_sources "${sources_without_tripwire}")"; then
    fail "no-tripwire: the checker accepted a listed site without a tripwire"
fi
printf '%s\n' "${output}" | /usr/bin/grep -qF 'Sources/SingleInstanceGuard.swift names the live Codex home without a tripwire' \
    || fail "no-tripwire: unexpected report: ${output}"
sources_missing_site="$(make_sources missing-site)"
/bin/rm -f "${sources_missing_site}/QuotaModels.swift"
if output="$(check_sources "${sources_missing_site}")"; then
    fail "missing-site: the checker accepted Sources without a listed file"
fi
printf '%s\n' "${output}" | /usr/bin/grep -qF 'Sources/QuotaModels.swift is missing' \
    || fail "missing-site: unexpected report: ${output}"

# --- The repository follows both rules ----------------------------------------
output="$(check_script "${PROJECT_DIR}/scripts/test_swift.sh")" \
    || fail "scripts/test_swift.sh compiles a Swift test without CODEX_MONITOR_TESTS:
${output}"
output="$(check_sources "${PROJECT_DIR}/Sources")" \
    || fail "Sources/ names the live Codex home outside its tripwired sites:
${output}"

echo "swift test codex home isolation: ok"
