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
# scripts/test_swift.sh therefore compiles only through one wrapper that adds the
# define. This check requires that wrapper, spelled exactly, once, and rejects
# any other line there that names swiftc or defines swiftc_test. Lines whose
# first non-blank character is # are ignored.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-test-define-lint.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT

WRAPPER='swiftc_test() { swiftc -D CODEX_MONITOR_TESTS "$@"; }'
# The swiftc command word, and any definition of the wrapper's name.
SWIFTC_WORD='(^|[^[:alnum:]_])swiftc([^[:alnum:]_]|$)'
WRAPPER_DEFINITION='(^|[^[:alnum:]_])swiftc_test[[:space:]]*\(|function[[:space:]]+swiftc_test([^[:alnum:]_]|$)'

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

# --- The repository follows the rule ------------------------------------------
output="$(check_script "${PROJECT_DIR}/scripts/test_swift.sh")" \
    || fail "scripts/test_swift.sh compiles a Swift test without CODEX_MONITOR_TESTS:
${output}"

echo "swift test codex home isolation: ok"
