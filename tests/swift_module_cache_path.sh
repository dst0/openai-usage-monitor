#!/bin/bash
# Regression test for the Swift module-cache spelling hazard.
#
# Implicitly built Clang modules record the absolute paths of the module files
# they import, spelled the way CLANG_MODULE_CACHE_PATH was spelled when they
# were built. Reusing that cache through another spelling of the same directory
# (for example `/tmp/x` after building it as `/private/tmp/x`) made
# swift-frontend report "module '_DarwinFoundation1' is defined in both ..."
# and crash during installation. scripts/install.sh and scripts/test_swift.sh
# must therefore resolve the variable to its physical spelling before their
# first swiftc call. See
# docs/leanings/2026-09-28-swift-cache-spelling-mismatch-isolated.md.
set -euo pipefail

# scripts/test_swift.sh runs this test before its first swiftc call; the
# end-to-end check below runs test_swift.sh again, which must not recurse.
[ -z "${CODEX_SWIFT_CACHE_TEST_NESTED:-}" ] || exit 0

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
HELPER="${PROJECT_DIR}/scripts/swift_module_cache.sh"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-module-cache-test.XXXXXX")"
TMP_ALIAS_ROOT=""
cleanup() {
    /bin/chmod -R u+w -- "${TEMP_ROOT}" 2>/dev/null || true
    /bin/rm -rf -- "${TEMP_ROOT}"
    if [ -n "${TMP_ALIAS_ROOT}" ]; then
        /bin/rm -rf -- "${TMP_ALIAS_ROOT}"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# The test itself compares physical spellings, so pin its own root first.
TEMP_ROOT="$(cd -P -- "${TEMP_ROOT}" && /bin/pwd -P)"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# --- Both scripts canonicalize before their first swiftc call ----------------
# A compile is any `swiftc` command word, also after `xcrun`, `!`, `$(`, `;`,
# `&&`, or a pipe. Comments and `command -v swiftc` lookups are not compiles.
SWIFTC_WORD='(^|[[:space:];&|({!]|\$\()(xcrun[[:space:]]+)?swiftc([[:space:]]|$)'
first_swiftc_line() {
    /usr/bin/grep -n -E "${SWIFTC_WORD}" "$1" \
        | /usr/bin/grep -v -E '^[0-9]+:[[:space:]]*#' \
        | /usr/bin/grep -v -E 'command -v swiftc' \
        | /usr/bin/head -n 1 | /usr/bin/cut -d: -f1 || true
}
line_of() {
    # line_of FILE ERE -> first matching line number, or empty
    /usr/bin/grep -n -m1 -E "$2" "$1" | /usr/bin/cut -d: -f1 || true
}
for script in scripts/install.sh scripts/test_swift.sh; do
    path="${PROJECT_DIR}/${script}"
    # Top level only: a call indented into a function or branch may never run.
    source_line="$(line_of "${path}" '^source "\$\{[A-Z_]+\}/(scripts/)?swift_module_cache\.sh"$')"
    call_line="$(line_of "${path}" '^canonicalize_clang_module_cache_path \|\| exit 1$')"
    swiftc_line="$(first_swiftc_line "${path}")"
    [ -n "${swiftc_line}" ] || fail "${script}: no swiftc invocation found"
    [ -n "${source_line}" ] || fail "${script}: does not source scripts/swift_module_cache.sh"
    [ -n "${call_line}" ] || fail "${script}: does not call canonicalize_clang_module_cache_path fail-closed at top level"
    [ "${source_line}" -lt "${call_line}" ] || fail "${script}: calls the cache helper before sourcing it"
    [ "${call_line}" -lt "${swiftc_line}" ] \
        || fail "${script}: first swiftc (line ${swiftc_line}) runs before the cache path is canonicalized (line ${call_line})"
done
# install.sh sources these helpers before the call; none of them may compile.
for helper in "${PROJECT_DIR}"/scripts/install_*.sh; do
    [ -z "$(first_swiftc_line "${helper}")" ] \
        || fail "${helper#"${PROJECT_DIR}"/} runs swiftc, which install.sh could reach before canonicalizing"
done

[ -f "${HELPER}" ] || fail "missing ${HELPER}"

# --- Helper semantics ---------------------------------------------------------
REAL="${TEMP_ROOT}/real"
ALIAS="${TEMP_ROOT}/alias"
/bin/mkdir -p "${REAL}"
/bin/ln -s real "${ALIAS}"

# run_helper ENV_ASSIGNMENT... -> prints "rc|value|set" from a clean subshell.
# The value is read from the child environment, so it must also be exported.
run_helper() {
    (
        unset CLANG_MODULE_CACHE_PATH
        for assignment in "$@"; do
            export "${assignment}"
        done
        # shellcheck source=../scripts/swift_module_cache.sh
        source "${HELPER}"
        rc=0
        canonicalize_clang_module_cache_path >/dev/null 2>"${TEMP_ROOT}/helper.err" || rc=$?
        if value="$(/usr/bin/printenv CLANG_MODULE_CACHE_PATH)"; then
            printf '%s|%s|set\n' "${rc}" "${value}"
        else
            printf '%s||unset\n' "${rc}"
        fi
    )
}
expect() {
    # expect LABEL ACTUAL EXPECTED
    [ "$2" = "$3" ] || fail "$1: expected '$3', got '$2'"
}

expect "unset stays unset" "$(run_helper)" "0||unset"
expect "empty stays empty" "$(run_helper CLANG_MODULE_CACHE_PATH=)" "0||set"
expect "physical path unchanged" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${REAL}")" "0|${REAL}|set"
expect "existing alias resolves" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}")" "0|${REAL}|set"
expect "missing directory behind alias is created and resolved" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/new/cache")" "0|${REAL}/new/cache|set"
[ -d "${REAL}/new/cache" ] || fail "helper did not create the missing cache directory"
expect "trailing slash is dropped" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/new/cache/")" "0|${REAL}/new/cache|set"

# A relative path resolves against the caller's directory, never CDPATH, so
# later `cd` calls in install.sh cannot move the cache between compile steps.
/bin/mkdir -p "${TEMP_ROOT}/cwd/rel" "${TEMP_ROOT}/elsewhere/rel"
actual="$(cd "${TEMP_ROOT}/cwd" && run_helper "CDPATH=${TEMP_ROOT}/elsewhere" CLANG_MODULE_CACHE_PATH=rel)"
expect "relative path uses the caller's directory" "${actual}" "0|${TEMP_ROOT}/cwd/rel|set"

# Fail closed on a path that cannot be a directory; the caller exits.
/usr/bin/printf 'not a directory\n' > "${TEMP_ROOT}/file"
actual="$(run_helper "CLANG_MODULE_CACHE_PATH=${TEMP_ROOT}/file")"
case "${actual}" in
    0\|*) fail "a regular file was accepted as the module cache: ${actual}" ;;
esac
expect "rejected path is left unchanged" "${actual#*|}" "${TEMP_ROOT}/file|set"
/usr/bin/grep -q 'CLANG_MODULE_CACHE_PATH' "${TEMP_ROOT}/helper.err" \
    || fail "rejection did not explain which variable was unusable"

# A symlink to a file is no better than the file itself.
/bin/ln -s file "${TEMP_ROOT}/file-link"
actual="$(run_helper "CLANG_MODULE_CACHE_PATH=${TEMP_ROOT}/file-link")"
case "${actual}" in
    0\|*) fail "a symlink to a regular file was accepted: ${actual}" ;;
esac

# Operands that `cd` would treat specially stay plain relative directories.
/bin/mkdir -p "${TEMP_ROOT}/odd"
actual="$(cd "${TEMP_ROOT}/odd" && OLDPWD="${TEMP_ROOT}/elsewhere" run_helper CLANG_MODULE_CACHE_PATH=-)"
expect "a directory named '-' is not OLDPWD" "${actual}" "0|${TEMP_ROOT}/odd/-|set"
actual="$(cd "${TEMP_ROOT}/odd" && run_helper CLANG_MODULE_CACHE_PATH=-P)"
expect "a leading dash is a name, not an option" "${actual}" "0|${TEMP_ROOT}/odd/-P|set"
actual="$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/with space")"
expect "spaces survive resolution" "${actual}" "0|${REAL}/with space|set"

# An empty unwritable cache would surface later as "this SDK is not supported
# by the compiler"; reject it up front with its real cause. A warm read-only
# cache still compiles what it holds, so it is kept with a warning. Root can
# write anyway, so these cases need an ordinary user.
if [ "$(/usr/bin/id -u)" != 0 ]; then
    /bin/mkdir -p "${REAL}/readonly-empty" "${REAL}/readonly-warm/HASH"
    /bin/chmod 555 "${REAL}/readonly-empty" "${REAL}/readonly-warm"
    actual="$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/readonly-empty")"
    case "${actual}" in
        0\|*) fail "an empty read-only cache directory was accepted: ${actual}" ;;
    esac
    expect "empty read-only path is left unchanged" "${actual#*|}" "${ALIAS}/readonly-empty|set"
    /usr/bin/grep -q 'not writable' "${TEMP_ROOT}/helper.err" \
        || fail "empty read-only rejection did not name the cause"
    actual="$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/readonly-warm")"
    expect "warm read-only cache is kept" "${actual}" "0|${REAL}/readonly-warm|set"
    /usr/bin/grep -q 'not writable' "${TEMP_ROOT}/helper.err" \
        || fail "warm read-only cache was kept without a warning"
    /bin/chmod 755 "${REAL}/readonly-empty" "${REAL}/readonly-warm"
fi

# The macOS /tmp symlink itself resolves to /private/tmp. Skip it where /tmp
# is not writable (for example a sandbox that only allows TMPDIR).
if [ -L /tmp ] && [ "$(cd -P /tmp && /bin/pwd -P)" = /private/tmp ] &&
    TMP_ALIAS_ROOT="$(/usr/bin/mktemp -d /tmp/codex-swift-module-cache-alias.XXXXXX 2>/dev/null)"; then
    name="${TMP_ALIAS_ROOT#/tmp/}"
    expect "/tmp alias resolves to /private/tmp" \
        "$(run_helper "CLANG_MODULE_CACHE_PATH=/tmp/${name}/cache")" "0|/private/tmp/${name}/cache|set"
fi

# --- The real test_swift.sh hands swiftc the physical path --------------------
# A stub swiftc records the cache path it receives and fails, which stops
# test_swift.sh at its first compile. install.sh cannot run here (it builds and
# installs), so the ordering checks above cover it.
/bin/mkdir -p "${TEMP_ROOT}/stub-bin"
/bin/cat > "${TEMP_ROOT}/stub-bin/swiftc" <<'STUB'
#!/bin/bash
/usr/bin/printf '%s\n' "${CLANG_MODULE_CACHE_PATH-<unset>}" >> "${CODEX_SWIFT_CACHE_TEST_LOG}"
exit 97
STUB
/bin/chmod 755 "${TEMP_ROOT}/stub-bin/swiftc"
: > "${TEMP_ROOT}/stub.log"
if PATH="${TEMP_ROOT}/stub-bin:${PATH}" \
    CODEX_SWIFT_CACHE_TEST_NESTED=1 \
    CODEX_SWIFT_CACHE_TEST_LOG="${TEMP_ROOT}/stub.log" \
    CLANG_MODULE_CACHE_PATH="${ALIAS}/e2e" \
    /bin/bash "${PROJECT_DIR}/scripts/test_swift.sh" > "${TEMP_ROOT}/e2e.out" 2>&1; then
    fail "test_swift.sh ignored the failing stub swiftc"
fi
expect "test_swift.sh gives its first swiftc the physical cache path" \
    "$(/usr/bin/head -n 1 "${TEMP_ROOT}/stub.log")" "${REAL}/e2e"

# --- Real compiler: a warm cache is reused, not rebuilt, through an alias ------
if [ "$(uname -s)" = Darwin ] && command -v swiftc >/dev/null 2>&1; then
    probe="${TEMP_ROOT}/probe.swift"
    /usr/bin/printf 'import Darwin\nlet pid: pid_t? = getpid()\nprint(pid ?? 0)\n' > "${probe}"
    compile() {
        # compile CACHE_PATH OUTPUT LOG
        CLANG_MODULE_CACHE_PATH="$1" swiftc -target "$(uname -m)-apple-macosx13.0" \
            -o "$2" "${probe}" > "$3" 2>&1
    }

    compile "${REAL}/cache" "${TEMP_ROOT}/warm" "${TEMP_ROOT}/warm.log" \
        || { /bin/cat "${TEMP_ROOT}/warm.log" >&2; fail "warming the module cache failed"; }
    [ -n "$(/usr/bin/find "${REAL}/cache" -name '*.pcm' -print)" ] \
        || fail "warm compile did not populate the module cache"
    # Module file names do not depend on the path spelling, so detect a rebuild
    # by modification time rather than by name.
    /usr/bin/touch "${TEMP_ROOT}/before-reuse"
    /bin/sleep 1

    resolved="$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/cache")"
    expect "alias of the warm cache resolves" "${resolved}" "0|${REAL}/cache|set"
    cache_path="${resolved#0|}"
    cache_path="${cache_path%|set}"
    compile "${cache_path}" "${TEMP_ROOT}/reuse" "${TEMP_ROOT}/reuse.log" \
        || { /bin/cat "${TEMP_ROOT}/reuse.log" >&2; fail "reusing the warm cache through the resolved alias failed"; }
    rebuilt="$(/usr/bin/find "${REAL}/cache" -name '*.pcm' -newer "${TEMP_ROOT}/before-reuse" -print)"
    [ -z "${rebuilt}" ] \
        || fail "reuse through the resolved path rebuilt module files: ${rebuilt}"
    "${TEMP_ROOT}/reuse" | /usr/bin/grep -Eq '^[0-9]+$' || fail "reused-cache binary did not run"

    # Informational control, run last so it cannot disturb the checks above:
    # does this toolchain still fail when the raw alias spelling is reused?
    # Swift 6.4 reports a duplicate module; Swift 5.10 (GitHub's macos-14
    # image) rejects the recorded module cache path instead.
    # `swiftc --version` leaves an empty swift-driver TemporaryDirectory.* in
    # TMPDIR (Swift 6.4), so give it this test's root instead.
    echo "control toolchain: $(TMPDIR="${TEMP_ROOT}/" swiftc --version 2>&1 | /usr/bin/head -n 1)"
    if compile "${ALIAS}/cache" "${TEMP_ROOT}/control" "${TEMP_ROOT}/control.log"; then
        echo "control: raw alias reuse compiled on this toolchain"
    elif /usr/bin/grep -q 'is defined in both' "${TEMP_ROOT}/control.log"; then
        echo "control: raw alias reuse reproduced the duplicate-module failure"
    elif /usr/bin/grep -q 'was compiled with module cache path' "${TEMP_ROOT}/control.log"; then
        echo "control: raw alias reuse reproduced the module-cache-path mismatch failure"
    else
        echo "control: raw alias reuse failed differently:"
        /usr/bin/grep -m3 'error' "${TEMP_ROOT}/control.log" || true
    fi
fi

echo "swift module cache path: ok"
