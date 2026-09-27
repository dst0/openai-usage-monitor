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

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
HELPER="${PROJECT_DIR}/scripts/swift_module_cache.sh"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-module-cache-test.XXXXXX")"
TMP_ALIAS_ROOT=""
cleanup() {
    /bin/rm -rf -- "${TEMP_ROOT}"
    if [ -n "${TMP_ALIAS_ROOT}" ]; then
        /bin/rm -rf -- "${TMP_ALIAS_ROOT}"
    fi
}
trap cleanup EXIT INT TERM
# The test itself compares physical spellings, so pin its own root first.
TEMP_ROOT="$(cd -P -- "${TEMP_ROOT}" && /bin/pwd -P)"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# --- Both scripts canonicalize before their first swiftc call ----------------
line_of() {
    # line_of FILE ERE -> first matching line number, or empty
    /usr/bin/grep -n -m1 -E "$2" "$1" | /usr/bin/cut -d: -f1 || true
}
for script in scripts/install.sh scripts/test_swift.sh; do
    path="${PROJECT_DIR}/${script}"
    source_line="$(line_of "${path}" '^[[:space:]]*source "\$\{[A-Z_]+\}/(scripts/)?swift_module_cache\.sh"$')"
    call_line="$(line_of "${path}" '^[[:space:]]*canonicalize_clang_module_cache_path \|\| exit 1$')"
    swiftc_line="$(line_of "${path}" '^[[:space:]]*swiftc([[:space:]]|$)')"
    [ -n "${swiftc_line}" ] || fail "${script}: no swiftc invocation found"
    [ -n "${source_line}" ] || fail "${script}: does not source scripts/swift_module_cache.sh"
    [ -n "${call_line}" ] || fail "${script}: does not call canonicalize_clang_module_cache_path fail-closed"
    [ "${source_line}" -lt "${call_line}" ] || fail "${script}: calls the cache helper before sourcing it"
    [ "${call_line}" -lt "${swiftc_line}" ] \
        || fail "${script}: first swiftc (line ${swiftc_line}) runs before the cache path is canonicalized (line ${call_line})"
done

[ -f "${HELPER}" ] || fail "missing ${HELPER}"

# --- Helper semantics ---------------------------------------------------------
REAL="${TEMP_ROOT}/real"
ALIAS="${TEMP_ROOT}/alias"
/bin/mkdir -p "${REAL}"
/bin/ln -s real "${ALIAS}"

# run_helper ENV_ASSIGNMENT... -> prints "rc|value|set" from a clean subshell
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
        if [ -n "${CLANG_MODULE_CACHE_PATH+x}" ]; then
            printf '%s|%s|set\n' "${rc}" "${CLANG_MODULE_CACHE_PATH}"
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

# The macOS /tmp symlink itself resolves to /private/tmp.
if [ -L /tmp ] && [ "$(cd -P /tmp && /bin/pwd -P)" = /private/tmp ]; then
    TMP_ALIAS_ROOT="$(/usr/bin/mktemp -d /tmp/codex-swift-module-cache-alias.XXXXXX)"
    name="${TMP_ALIAS_ROOT#/tmp/}"
    expect "/tmp alias resolves to /private/tmp" \
        "$(run_helper "CLANG_MODULE_CACHE_PATH=/tmp/${name}/cache")" "0|/private/tmp/${name}/cache|set"
fi

# --- Real compiler: a warm cache is reused, not rebuilt, through an alias ------
if [ "$(uname -s)" = Darwin ] && command -v swiftc >/dev/null 2>&1; then
    probe="${TEMP_ROOT}/probe.swift"
    /usr/bin/printf 'import Darwin\nlet pid: pid_t? = getpid()\nprint(pid ?? 0)\n' > "${probe}"
    compile() {
        # compile CACHE_PATH OUTPUT LOG
        CLANG_MODULE_CACHE_PATH="$1" swiftc -target "$(uname -m)-apple-macosx13.0" \
            -o "$2" "${probe}" > "$3" 2>&1
    }
    pcm_inventory() {
        /usr/bin/find "${REAL}/cache" -name '*.pcm' -print | /usr/bin/sort
    }

    compile "${REAL}/cache" "${TEMP_ROOT}/warm" "${TEMP_ROOT}/warm.log" \
        || { /bin/cat "${TEMP_ROOT}/warm.log" >&2; fail "warming the module cache failed"; }
    [ -n "$(pcm_inventory)" ] || fail "warm compile did not populate the module cache"
    before="$(pcm_inventory)"

    resolved="$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/cache")"
    expect "alias of the warm cache resolves" "${resolved}" "0|${REAL}/cache|set"
    cache_path="${resolved#0|}"
    cache_path="${cache_path%|set}"
    compile "${cache_path}" "${TEMP_ROOT}/reuse" "${TEMP_ROOT}/reuse.log" \
        || { /bin/cat "${TEMP_ROOT}/reuse.log" >&2; fail "reusing the warm cache through the resolved alias failed"; }
    [ "$(pcm_inventory)" = "${before}" ] \
        || fail "the resolved alias rebuilt modules instead of reusing the warm cache"
    "${TEMP_ROOT}/reuse" | /usr/bin/grep -Eq '^[0-9]+$' || fail "reused-cache binary did not run"

    # Informational control, run last so it cannot disturb the checks above:
    # does this toolchain still fail when the raw alias spelling is reused?
    if compile "${ALIAS}/cache" "${TEMP_ROOT}/control" "${TEMP_ROOT}/control.log"; then
        echo "control: raw alias reuse compiled on this toolchain"
    elif /usr/bin/grep -q 'is defined in both' "${TEMP_ROOT}/control.log"; then
        echo "control: raw alias reuse reproduced the duplicate-module failure"
    else
        echo "control: raw alias reuse failed differently:"
        /usr/bin/grep -m3 'error' "${TEMP_ROOT}/control.log" || true
    fi
fi

echo "swift module cache path: ok"
