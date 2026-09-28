#!/bin/bash
# Regression test for the Swift module-cache path hazard.
#
# Implicitly built Clang modules record the absolute paths of the module files
# they import, spelled the way the module cache path was spelled when they were
# built. Reusing that cache through another path fails: another spelling of the
# same directory (`/tmp/x` after `/private/tmp/x`), or a copied or moved cache.
# scripts/install.sh and scripts/test_swift.sh therefore never compile into
# CLANG_MODULE_CACHE_PATH itself. Before their first swiftc call,
# scripts/swift_module_cache.sh points it at a subdirectory of its physical
# path, named after a checksum of that path, which only these scripts use. See
# docs/leanings/2026-09-28-swift-cache-spelling-mismatch-isolated.md and
# docs/leanings/2026-09-28-swift-cache-helper-owns-a-path-keyed-subdirectory.md.
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
# Compiler runs get this test's own TMPDIR: `swiftc --version` leaves a
# swift-driver TemporaryDirectory.* behind (Swift 6.4).
COMPILER_TMPDIR="${TEMP_ROOT}/compiler-tmp/"
/bin/mkdir -p "${COMPILER_TMPDIR}"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}
expect() {
    # expect LABEL ACTUAL EXPECTED
    [ "$2" = "$3" ] || fail "$1: expected '$3', got '$2'"
}

[ -f "${HELPER}" ] || fail "missing ${HELPER}"

REAL="${TEMP_ROOT}/real"
ALIAS="${TEMP_ROOT}/alias"
/bin/mkdir -p "${REAL}"
/bin/ln -s real "${ALIAS}"

# scripts_cache PHYSICAL_DIR -> the subdirectory the helper must choose.
scripts_cache() {
    local checksum
    checksum="$(/usr/bin/printf '%s' "$1" | /usr/bin/cksum)"
    /usr/bin/printf '%s/codex-monitor-swift-%s' "${1%/}" "${checksum%% *}"
}

# --- 1. Real compiler: the helper's result compiles where the raw path fails --
# Runs first, so a helper that stops isolating the scripts' cache fails here on
# a compile's exit status, not on a string comparison.
if [ "$(uname -s)" = Darwin ]; then
    command -v swiftc >/dev/null 2>&1 || fail "swiftc is required on macOS"
    probe="${TEMP_ROOT}/probe.swift"
    /usr/bin/printf 'import Darwin\nlet pid: pid_t? = getpid()\nprint(pid ?? 0)\n' > "${probe}"
    compile_raw() {
        # compile_raw CACHE_PATH OUTPUT LOG
        CLANG_MODULE_CACHE_PATH="$1" TMPDIR="${COMPILER_TMPDIR}" \
            swiftc -target "$(uname -m)-apple-macosx13.0" -o "$2" "${probe}" > "$3" 2>&1
    }
    compile_via_helper() {
        # compile_via_helper REQUESTED OUTPUT LOG -> compiles with whatever the
        # helper exported, which it also writes to LOG.value.
        (
            export CLANG_MODULE_CACHE_PATH="$1"
            # shellcheck source=../scripts/swift_module_cache.sh
            source "${HELPER}"
            canonicalize_clang_module_cache_path > "$3" 2>&1 || exit 1
            /usr/bin/printf '%s' "${CLANG_MODULE_CACHE_PATH}" > "$3.value"
            TMPDIR="${COMPILER_TMPDIR}" \
                swiftc -target "$(uname -m)-apple-macosx13.0" -o "$2" "${probe}" >> "$3" 2>&1
        )
    }
    module_files() {
        # Inode, modification time, size, and name of every module file: a
        # rebuild keeps the names, so compare the files themselves.
        /usr/bin/find "$1" -type f \( -name '*.pcm' -o -name '*.swiftmodule' \) \
            -exec /usr/bin/stat -f '%i %m %z %N' {} + | /usr/bin/sort
    }
    show_log() {
        /usr/bin/sed -n '1,20p' "$1" >&2
    }

    # A tool outside the scripts warms the directory through the alias.
    compile_raw "${ALIAS}/cache" "${TEMP_ROOT}/warm" "${TEMP_ROOT}/warm.log" \
        || { show_log "${TEMP_ROOT}/warm.log"; fail "warming the outside cache through the alias failed"; }
    [ -n "$(/usr/bin/find "${REAL}/cache" -name '*.pcm' -print)" ] \
        || fail "warm compile did not populate the module cache"

    # Control: the raw physical spelling must fail on that cache, or the
    # checks below prove nothing on this toolchain. Swift 6.4 reports a
    # duplicate module; Swift 5.10 (GitHub's macos-14 image) reports the
    # recorded module cache path and a missing SwiftShims.
    toolchain="$(TMPDIR="${COMPILER_TMPDIR}" swiftc --version 2>&1 | /usr/bin/head -n 1)"
    if compile_raw "${REAL}/cache" "${TEMP_ROOT}/control" "${TEMP_ROOT}/control.log"; then
        fail "raw reuse through another spelling compiled on ${toolchain}; this test no longer reproduces the hazard, so re-verify it and update the test and docs"
    fi
    signature="$(/usr/bin/grep -o -m1 -E "is defined in both|was compiled with module cache path|missing required module 'SwiftShims'" \
        "${TEMP_ROOT}/control.log" | /usr/bin/head -n 1 || true)"
    [ -n "${signature}" ] \
        || { show_log "${TEMP_ROOT}/control.log"; fail "raw reuse through another spelling failed with an unrecognized error on ${toolchain}"; }
    echo "control: ${toolchain}: raw reuse through another spelling failed with '${signature}'"

    # The scripts' path: given the alias the outside tool used, then the
    # physical spelling. Both compiles must pass before any value is checked.
    compile_via_helper "${ALIAS}/cache" "${TEMP_ROOT}/first" "${TEMP_ROOT}/first.log" \
        || { show_log "${TEMP_ROOT}/first.log"; fail "compiling with the helper's cache path failed next to a cache warmed through an alias"; }
    first_cache="$(/bin/cat "${TEMP_ROOT}/first.log.value")"
    before="$(module_files "${first_cache}")"
    /bin/sleep 1
    compile_via_helper "${REAL}/cache/" "${TEMP_ROOT}/reuse" "${TEMP_ROOT}/reuse.log" \
        || { show_log "${TEMP_ROOT}/reuse.log"; fail "reusing the helper's cache through another spelling failed"; }
    after="$(module_files "${first_cache}")"

    expect "the helper uses the subdirectory keyed by the physical path" \
        "${first_cache}" "$(scripts_cache "${REAL}/cache")"
    expect "another spelling reaches the same subdirectory" \
        "$(/bin/cat "${TEMP_ROOT}/reuse.log.value")" "${first_cache}"
    [ -n "${before}" ] || fail "the helper's cache holds no module files after a compile"
    [ "${before}" = "${after}" ] || fail "reuse through another spelling rebuilt module files:
$(/usr/bin/diff <(echo "${before}") <(echo "${after}") || true)"
    for binary in first reuse; do
        "${TEMP_ROOT}/${binary}" | /usr/bin/grep -Eq '^[0-9]+$' || fail "the ${binary} binary did not run"
    done
else
    echo "skipped real compiler checks: not macOS"
fi

# --- 2. The real test_swift.sh gives every compile the scripts' cache --------
# A stub swiftc first on PATH records the cache path of each call and writes a
# passing stub binary to its -o path, so the whole script runs. Any unset or
# reassignment between compiles shows up in the log.
/bin/mkdir -p "${TEMP_ROOT}/stub-bin"
/bin/cat > "${TEMP_ROOT}/stub-bin/swiftc" <<'STUB'
#!/bin/bash
/usr/bin/printf '%s\n' "${CLANG_MODULE_CACHE_PATH-<unset>}" >> "${CODEX_SWIFT_CACHE_TEST_LOG}"
output=""
while [ "$#" -gt 0 ]; do
    if [ "$1" = -o ] && [ "$#" -gt 1 ]; then
        output="$2"
        shift
    fi
    shift
done
if [ -n "${output}" ]; then
    /usr/bin/printf '#!/bin/sh\nexit 0\n' > "${output}"
    /bin/chmod 755 "${output}"
fi
STUB
/bin/chmod 755 "${TEMP_ROOT}/stub-bin/swiftc"
: > "${TEMP_ROOT}/stub.log"
PATH="${TEMP_ROOT}/stub-bin:${PATH}" \
    CODEX_SWIFT_CACHE_TEST_NESTED=1 \
    CODEX_SWIFT_CACHE_TEST_LOG="${TEMP_ROOT}/stub.log" \
    CLANG_MODULE_CACHE_PATH="${ALIAS}/e2e" \
    /bin/bash "${PROJECT_DIR}/scripts/test_swift.sh" > "${TEMP_ROOT}/e2e.out" 2>&1 \
    || { /usr/bin/tail -n 20 "${TEMP_ROOT}/e2e.out" >&2; fail "test_swift.sh failed with a passing stub swiftc"; }
[ -s "${TEMP_ROOT}/stub.log" ] || fail "test_swift.sh never ran swiftc from PATH"
expected="$(scripts_cache "${REAL}/e2e")"
while IFS= read -r seen; do
    expect "every swiftc run by test_swift.sh gets the scripts' cache" "${seen}" "${expected}"
done < "${TEMP_ROOT}/stub.log"

# --- 3. Helper semantics -------------------------------------------------------
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
        canonicalize_clang_module_cache_path > "${TEMP_ROOT}/helper.out" 2> "${TEMP_ROOT}/helper.err" || rc=$?
        # The trailing "x" keeps a value that ends in a newline intact.
        if value="$(/usr/bin/printenv CLANG_MODULE_CACHE_PATH && echo x)"; then
            value="${value%x}"
            printf '%s|%s|set\n' "${rc}" "${value%$'\n'}"
        else
            printf '%s||unset\n' "${rc}"
        fi
    )
}
accepted() {
    # accepted PHYSICAL_DIR -> the run_helper result that selects its subdirectory
    printf '0|%s|set' "$(scripts_cache "$1")"
}
expect_rejected() {
    # expect_rejected LABEL REQUESTED [CAUSE]: CAUSE is a fixed string that
    # must appear in the error, which also quotes the path.
    local actual
    actual="$(run_helper "CLANG_MODULE_CACHE_PATH=$2")"
    case "${actual}" in
        0\|*) fail "$1 was accepted: ${actual}" ;;
    esac
    expect "$1 leaves the variable unchanged" "${actual#*|}" "$2|set"
    /usr/bin/grep -qF "${3:-CLANG_MODULE_CACHE_PATH is not a usable directory}" "${TEMP_ROOT}/helper.err" \
        || fail "$1: the error does not say why: $(/bin/cat "${TEMP_ROOT}/helper.err")"
}

expect "unset stays unset" "$(run_helper)" "0||unset"
expect "empty stays empty" "$(run_helper CLANG_MODULE_CACHE_PATH=)" "0||set"
expect "physical path" "$(run_helper "CLANG_MODULE_CACHE_PATH=${REAL}")" "$(accepted "${REAL}")"
[ ! -s "${TEMP_ROOT}/helper.out" ] || fail "the helper wrote to stdout: $(/bin/cat "${TEMP_ROOT}/helper.out")"
/usr/bin/grep -q 'Swift module cache:' "${TEMP_ROOT}/helper.err" \
    || fail "the helper did not report the cache it chose on stderr"
[ -d "$(scripts_cache "${REAL}")" ] || fail "the helper did not create its subdirectory"
expect "the helper writes nothing into the cache itself" "$(/bin/ls -A "$(scripts_cache "${REAL}")")" ""
expect "alias" "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}")" "$(accepted "${REAL}")"
expect "missing directory behind an alias is created" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/new/cache")" "$(accepted "${REAL}/new/cache")"
[ -d "${REAL}/new/cache" ] || fail "helper did not create the missing cache directory"
expect "trailing slash" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/new/cache/")" "$(accepted "${REAL}/new/cache")"
expect "doubled slashes and dot components" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=/${ALIAS}//new/./cache")" "$(accepted "${REAL}/new/cache")"
upper="$(/usr/bin/printf '%s' "${REAL}/new/cache" | /usr/bin/tr '[:lower:]' '[:upper:]')"
if [ "${upper}" != "${REAL}/new/cache" ] && [ -d "${upper}" ]; then
    expect "another letter case on a case-insensitive volume" \
        "$(run_helper "CLANG_MODULE_CACHE_PATH=${upper}")" "$(accepted "${REAL}/new/cache")"
fi
if [ -d "/System/Volumes/Data${REAL}" ]; then
    expect "the /System/Volumes/Data firmlink spelling" \
        "$(run_helper "CLANG_MODULE_CACHE_PATH=/System/Volumes/Data${REAL}/new/cache")" \
        "$(accepted "${REAL}/new/cache")"
fi

# A copied or moved cache records its old path, so it gets a new subdirectory.
/bin/mkdir -p "${REAL}/movable"
expect "movable" "$(run_helper "CLANG_MODULE_CACHE_PATH=${REAL}/movable")" "$(accepted "${REAL}/movable")"
/bin/cp -R "${REAL}/movable" "${REAL}/copied"
/bin/mv "${REAL}/movable" "${REAL}/moved"
for moved in copied moved; do
    actual="$(run_helper "CLANG_MODULE_CACHE_PATH=${REAL}/${moved}")"
    expect "${moved} cache" "${actual}" "$(accepted "${REAL}/${moved}")"
    [ "${actual}" != "$(accepted "${REAL}/movable")" ] || fail "${moved} cache reused its old subdirectory name"
done

# A relative path resolves against the caller's directory, never CDPATH, so
# later `cd` calls in install.sh cannot move the cache between compile steps.
/bin/mkdir -p "${TEMP_ROOT}/cwd/rel" "${TEMP_ROOT}/elsewhere/rel"
actual="$(cd "${TEMP_ROOT}/cwd" && run_helper "CDPATH=${TEMP_ROOT}/elsewhere" CLANG_MODULE_CACHE_PATH=rel)"
expect "relative path uses the caller's directory" "${actual}" "$(accepted "${TEMP_ROOT}/cwd/rel")"

# Operands that `cd` would treat specially stay plain relative directories.
/bin/mkdir -p "${TEMP_ROOT}/odd"
actual="$(cd "${TEMP_ROOT}/odd" && OLDPWD="${TEMP_ROOT}/elsewhere" run_helper CLANG_MODULE_CACHE_PATH=-)"
expect "a directory named '-' is not OLDPWD" "${actual}" "$(accepted "${TEMP_ROOT}/odd/-")"
expect "'-' leaves OLDPWD alone" "$(/bin/ls -A "${TEMP_ROOT}/elsewhere")" rel
actual="$(cd "${TEMP_ROOT}/odd" && run_helper CLANG_MODULE_CACHE_PATH=-P)"
expect "a leading dash is a name, not an option" "${actual}" "$(accepted "${TEMP_ROOT}/odd/-P")"
expect "spaces survive resolution" \
    "$(run_helper "CLANG_MODULE_CACHE_PATH=${ALIAS}/with space")" "$(accepted "${REAL}/with space")"

# Fail closed on anything that is not a usable directory; the caller exits.
/usr/bin/printf 'not a directory\n' > "${TEMP_ROOT}/file"
expect_rejected "a regular file" "${TEMP_ROOT}/file"
/bin/ln -s file "${TEMP_ROOT}/file-link"
expect_rejected "a symlink to a regular file" "${TEMP_ROOT}/file-link"
/bin/ln -s loop "${TEMP_ROOT}/loop"
expect_rejected "a symlink loop" "${TEMP_ROOT}/loop"
actual="$(cd "${TEMP_ROOT}/odd" && run_helper "CLANG_MODULE_CACHE_PATH=~/codex-module-cache")"
expect "an unexpanded '~' is rejected and left unchanged" "${actual}" "1|~/codex-module-cache|set"
/usr/bin/grep -qF 'Use "${HOME}/..."' "${TEMP_ROOT}/helper.err" || fail "the '~' rejection does not suggest \${HOME}"
[ ! -e "${TEMP_ROOT}/odd/~" ] || fail "the helper created a directory named '~'"
# Newlines: command substitution drops a trailing one, which would name the
# sibling directory without it.
/bin/mkdir -p "${REAL}/nl" "${REAL}/nl"$'\n' "${REAL}/a"$'\n'"b"
expect_rejected "a path ending in a newline" "${REAL}/nl"$'\n' 'contains a newline;'
expect_rejected "a path containing a newline" "${REAL}/a"$'\n'"b" 'contains a newline;'
/bin/ln -s "nl"$'\n' "${REAL}/nl-link"
expect_rejected "a symlink to a directory whose name ends in a newline" "${REAL}/nl-link" 'did not resolve to itself'
/bin/ln -s "a"$'\n'"b" "${REAL}/ab-link"
expect_rejected "a symlink to a directory whose name contains a newline" "${REAL}/ab-link" 'resolves to a path that contains a newline'
/usr/bin/printf 'occupied\n' > "$(scripts_cache "${REAL}/nl")"
expect_rejected "a file where the scripts' subdirectory belongs" "${REAL}/nl" 'is not a plain directory'

# A failing checksum must stop the helper rather than name a shared directory.
/bin/cp "${HELPER}" "${TEMP_ROOT}/helper-failing-cksum.sh"
/usr/bin/printf '#!/bin/sh\nexit 1\n' > "${TEMP_ROOT}/fake-cksum"
/bin/chmod 755 "${TEMP_ROOT}/fake-cksum"
/usr/bin/sed -i '' "s#/usr/bin/cksum#${TEMP_ROOT}/fake-cksum#g" "${TEMP_ROOT}/helper-failing-cksum.sh"
! /usr/bin/grep -q '/usr/bin/cksum' "${TEMP_ROOT}/helper-failing-cksum.sh" \
    || fail "the failing-cksum copy still runs the real cksum"
actual="$(HELPER="${TEMP_ROOT}/helper-failing-cksum.sh" run_helper "CLANG_MODULE_CACHE_PATH=${REAL}")"
expect "a failing checksum is rejected" "${actual}" "1|${REAL}|set"
/usr/bin/grep -qF 'Could not name the Swift module cache' "${TEMP_ROOT}/helper.err" || fail "the checksum failure is not explained"

# The scripts' subdirectory must be a plain directory they can write. A cache
# the compiler cannot write fails on the first module it lacks, possibly as
# "this SDK is not supported by the compiler"; see the learnings above.
/bin/mkdir -p "${REAL}/linked" "${REAL}/elsewhere-cache"
/bin/ln -s ../elsewhere-cache "$(scripts_cache "${REAL}/linked")"
expect_rejected "a symlinked scripts' subdirectory" "${REAL}/linked" 'is not a plain directory'
if [ "$(/usr/bin/id -u)" != 0 ]; then
    # Root can write anyway, so these cases need an ordinary user.
    /bin/mkdir -p "${REAL}/ro-parent" "${REAL}/ro-subdir" "${REAL}/ro-parent-ok" "${REAL}/unsearchable"
    /bin/mkdir -p "$(scripts_cache "${REAL}/ro-subdir")/HASH" "$(scripts_cache "${REAL}/ro-parent-ok")" \
        "$(scripts_cache "${REAL}/unsearchable")"
    /bin/chmod 555 "${REAL}/ro-parent" "$(scripts_cache "${REAL}/ro-subdir")" "${REAL}/ro-parent-ok"
    /bin/chmod 600 "$(scripts_cache "${REAL}/unsearchable")"
    expect_rejected "a read-only directory" "${REAL}/ro-parent" 'cannot be written'
    expect_rejected "a warm read-only scripts' subdirectory" "${ALIAS}/ro-subdir" 'cannot be written'
    expect_rejected "a writable but unsearchable scripts' subdirectory" "${REAL}/unsearchable" 'cannot be written'
    expect "a read-only parent of a writable scripts' subdirectory" \
        "$(run_helper "CLANG_MODULE_CACHE_PATH=${REAL}/ro-parent-ok")" "$(accepted "${REAL}/ro-parent-ok")"
    /bin/chmod 755 "${REAL}/ro-parent" "$(scripts_cache "${REAL}/ro-subdir")" "${REAL}/ro-parent-ok" \
        "$(scripts_cache "${REAL}/unsearchable")"
fi
# A sandbox that denies writes the permission bits allow, as the agent sandbox
# did for the default cache, is rejected too, without a probe file.
if /usr/bin/sandbox-exec -p '(version 1)(allow default)' /usr/bin/true 2>/dev/null; then
    /bin/mkdir -p "$(scripts_cache "${REAL}/sandboxed")"
    actual="$(/usr/bin/sandbox-exec -p "(version 1)(allow default)(deny file-write* (subpath \"${REAL}/sandboxed\"))" \
        /bin/bash -c 'source "$1" && CLANG_MODULE_CACHE_PATH="$2" canonicalize_clang_module_cache_path 2>&1; echo "rc=$?"' \
        _ "${HELPER}" "${REAL}/sandboxed")"
    case "${actual}" in
        *'cannot be written'*'rc=1') ;;
        *) fail "a sandbox-denied cache was not rejected: ${actual}" ;;
    esac
else
    echo "skipped the sandbox case: sandbox-exec is unavailable"
fi

# The macOS /tmp symlink itself resolves to /private/tmp. Skip it where /tmp
# is not writable (for example a sandbox that only allows TMPDIR).
if [ -L /tmp ] && [ "$(cd -P /tmp && /bin/pwd -P)" = /private/tmp ] &&
    TMP_ALIAS_ROOT="$(/usr/bin/mktemp -d /tmp/codex-swift-module-cache-alias.XXXXXX 2>/dev/null)"; then
    name="${TMP_ALIAS_ROOT#/tmp/}"
    expect "/tmp alias resolves to /private/tmp" \
        "$(run_helper "CLANG_MODULE_CACHE_PATH=/tmp/${name}/cache")" "$(accepted "/private/tmp/${name}/cache")"
fi

# --- 4. install.sh, which cannot run here, calls the helper before compiling --
# bash prints a parsed script back with comments removed, one command per line,
# and top-level commands indented by exactly four spaces. test_swift.sh gets
# the same scan, and section 2 sees every compile it makes.
canonical() {
    /bin/bash -c 'body="$(/bin/cat -- "$1")" && eval "__codex_canonical() {
${body}
}" && declare -f __codex_canonical' _ "$1" | /usr/bin/sed '1,2d;$d'
}
# A compiler command word, also as ${SWIFTC}, "swiftc", or /usr/bin/swiftc,
# in any letter case; not a file name such as main.swift or
# swift_module_cache.sh.
COMPILER_WORD='(^|[^[:alnum:]_.-])(swift-frontend|swiftc|swift|xcrun|clang\+\+|clang)([^[:alnum:]_.-]|$)'
# After the call, a compile must be a plain `swiftc -...` command, optionally
# with variable assignments in front, so the PATH stub in section 2 sees it.
PLAIN_SWIFTC='^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*=("[^"]*"|[^[:space:]"]*)[[:space:]]+)*swiftc -'
names_compiler() {
    local status=1
    shopt -s nocasematch
    [[ $1 =~ ${COMPILER_WORD} ]] && status=0
    shopt -u nocasematch
    return "${status}"
}
plain_echo() {
    [[ $1 =~ ^[[:space:]]*echo[[:space:]] ]] && [[ $1 != *'$('* ]] && [[ $1 != *'`'* ]]
}
repo_script_sourced() {
    # repo_script_sourced LINE -> the scripts/ file name a source line loads
    /usr/bin/sed -nE \
        -e 's#^[[:space:]]*(source|\.) "\$\{(PROJECT_DIR|REPO_DIR)\}/scripts/([A-Za-z0-9_.-]+\.sh)";?$#\3#p' \
        -e 's#^[[:space:]]*(source|\.) "\$\{SCRIPT_DIR\}/([A-Za-z0-9_.-]+\.sh)";?$#\2#p' <<< "$1"
}
check_sourced_script() {
    # check_sourced_script LABEL FILE: must not compile or touch the cache.
    local text line
    text="$(canonical "$2")" || fail "$1: bash could not parse it"
    while IFS= read -r line; do
        plain_echo "${line}" && continue
        ! names_compiler "${line}" || fail "$1 can reach a compiler: ${line}"
        [[ ${line} != *CLANG_MODULE_CACHE_PATH* && ${line} != *canonicalize_clang_module_cache_path* ]] \
            || fail "$1 touches the module cache variable or the helper: ${line}"
    done <<< "${text}"
}
check_script() {
    local script="$1" text call_index="" source_index="" source_count=0 call_count=0 index=0 line sourced
    text="$(canonical "${PROJECT_DIR}/${script}")" || fail "${script}: bash could not parse it"
    while IFS= read -r line; do
        index=$((index + 1))
        if [[ ${line} == *swift_module_cache.sh* ]]; then
            source_count=$((source_count + 1))
            [[ ${line} =~ ^\ {4}source\ \"\$\{[A-Z_]+\}/(scripts/)?swift_module_cache\.sh\"\;?$ ]] && source_index="${index}"
        fi
        if [[ ${line} == *canonicalize_clang_module_cache_path* ]]; then
            call_count=$((call_count + 1))
            [[ ${line} =~ ^\ {4}canonicalize_clang_module_cache_path\ \|\|\ exit\ 1\;?$ ]] && call_index="${index}"
        fi
        [[ ${line} != *CLANG_MODULE_CACHE_PATH* ]] \
            || fail "${script}: sets, unsets, or passes CLANG_MODULE_CACHE_PATH itself: ${line}"
    done <<< "${text}"
    expect "${script}: sources the cache helper once" "${source_count}" 1
    [ -n "${source_index}" ] || fail "${script}: does not source scripts/swift_module_cache.sh at top level"
    expect "${script}: names the helper function once, in its call" "${call_count}" 1
    [ -n "${call_index}" ] || fail "${script}: does not call canonicalize_clang_module_cache_path fail-closed at top level"
    [ "${source_index}" -lt "${call_index}" ] || fail "${script}: calls the cache helper before sourcing it"
    index=0
    while IFS= read -r line; do
        index=$((index + 1))
        sourced="$(repo_script_sourced "${line}")"
        if [ -n "${sourced}" ] && [ "${sourced}" != swift_module_cache.sh ]; then
            [ -f "${PROJECT_DIR}/scripts/${sourced}" ] || fail "${script}: sources missing scripts/${sourced}"
            check_sourced_script "scripts/${sourced} (sourced by ${script})" "${PROJECT_DIR}/scripts/${sourced}"
        fi
        if [ "${index}" -lt "${call_index}" ]; then
            # Before the call: no compile, no cd, only the swiftc presence check.
            [[ ! ${line} =~ ^[[:space:]]*(cd|pushd)([[:space:]]|\;|$) ]] \
                || fail "${script}: changes directory before resolving a relative cache path: ${line}"
            if [[ ${line} =~ ^[[:space:]]*(source|\.)[[:space:]] ]] && [ -z "${sourced}" ]; then
                fail "${script}: sources an unchecked file before resolving the cache path: ${line}"
            fi
            plain_echo "${line}" && continue
            [ "${line}" = '    if ! command -v swiftc > /dev/null 2>&1; then' ] && continue
            ! names_compiler "${line}" \
                || fail "${script}: can reach a compiler before the cache path is resolved: ${line}"
        elif names_compiler "${line}" && ! plain_echo "${line}"; then
            [[ ${line} =~ ${PLAIN_SWIFTC} ]] \
                || fail "${script}: compiles other than through a plain swiftc command: ${line}"
        fi
    done <<< "${text}"
}

# Prove the scan on known good and bad scripts before trusting it.
bad_dir="${TEMP_ROOT}/scan-samples"
/bin/mkdir -p "${bad_dir}/scripts"
/usr/bin/printf 'compile_helper() {\n    swiftc -o x y.swift\n}\n' > "${bad_dir}/scripts/compiling_lib.sh"
/usr/bin/printf 'quiet_helper() {\n    echo "no Swift here"\n}\n' > "${bad_dir}/scripts/quiet_lib.sh"
sample() {
    # sample NAME BEFORE_CALL AFTER_CALL -> writes scripts/NAME.sh
    /usr/bin/printf '#!/bin/bash\nsource "${PROJECT_DIR}/scripts/swift_module_cache.sh"\n%s\ncanonicalize_clang_module_cache_path || exit 1\n%s\nswiftc -o x y.swift\n' \
        "$2" "$3" > "${bad_dir}/scripts/$1.sh"
}
scan_accepts() {
    (PROJECT_DIR="${bad_dir}"; check_script "scripts/$1.sh") 2>/dev/null
}
sample good 'source "${PROJECT_DIR}/scripts/quiet_lib.sh"' 'FOO="a b" swiftc -o x y.swift'
scan_accepts good || fail "the ordering scan rejected a correct script: $( (PROJECT_DIR="${bad_dir}"; check_script scripts/good.sh) 2>&1 )"
bad_case() {
    sample "$@"
    ! scan_accepts "$1" || fail "the ordering scan accepted a script with: ${2} / ${3}"
}
bad_case xcrun 'xcrun swiftc -o x y.swift' ':'
bad_case variable 'SWIFTC=swiftc' ':'
bad_case absolute '/usr/bin/swiftc -o x y.swift' ':'
bad_case env-prefix 'FOO=1 swiftc -o x y.swift' ':'
bad_case timed 'time swiftc -o x y.swift' ':'
bad_case swift-driver 'swift build' ':'
bad_case echo-substitution 'echo "$(swiftc -o x y.swift)"' ':'
bad_case compile-in-function 'build() { swiftc -o x y.swift; }' ':'
bad_case cd-first 'cd /' ':'
bad_case unchecked-source '. "${HOME}/env.sh"' ':'
bad_case compiling-source 'source "${PROJECT_DIR}/scripts/compiling_lib.sh"' ':'
bad_case unset-after ':' 'unset CLANG_MODULE_CACHE_PATH'
bad_case reassign-after ':' 'export CLANG_MODULE_CACHE_PATH=/tmp/x'
bad_case redefine-after ':' 'canonicalize_clang_module_cache_path() { :; }'
bad_case xcrun-after ':' 'xcrun swiftc -o x y.swift'
bad_case absolute-after ':' '/usr/bin/swiftc -o x y.swift'
bad_case variable-after ':' '"${SWIFTC}" -o x y.swift'
/usr/bin/printf '#!/bin/bash\nsource "${PROJECT_DIR}/scripts/swift_module_cache.sh"\nif true; then\n    canonicalize_clang_module_cache_path || exit 1\nfi\nswiftc -o x y.swift\n' \
    > "${bad_dir}/scripts/nested.sh"
! scan_accepts nested || fail "the ordering scan accepted a call nested in a block"
/usr/bin/printf '#!/bin/bash\nsource "${PROJECT_DIR}/scripts/swift_module_cache.sh"\nswiftc -o x y.swift\ncanonicalize_clang_module_cache_path || exit 1\n' \
    > "${bad_dir}/scripts/late.sh"
! scan_accepts late || fail "the ordering scan accepted a compile before the call"

for script in scripts/install.sh scripts/test_swift.sh; do
    check_script "${script}"
done

echo "swift module cache path: ok"
