#!/bin/bash
# Sourced by scripts/install.sh and scripts/test_swift.sh before their first
# swiftc call.
#
# Implicitly built Clang modules (.pcm) record the absolute paths of the module
# files they import, spelled the way CLANG_MODULE_CACHE_PATH was spelled when
# they were built. Reusing a warm cache through another spelling of the same
# directory, such as /tmp/x after building it as /private/tmp/x (/tmp is a
# symlink on macOS), makes swift-frontend load one module under two names:
# "module '_DarwinFoundation1' is defined in both ...", followed by a crash.
# Resolving the variable to its physical spelling keeps every run of these
# scripts on one spelling. It also pins a relative path to the caller's
# directory, so the installer's later `cd` calls cannot move the cache between
# compile steps. An unset or empty variable is left alone, as before.
#
# A cache the compiler cannot write fails as soon as a module must be built,
# with the misleading "this SDK is not supported by the compiler". An empty
# unwritable directory can never work, so it is rejected here with its real
# cause. A warm read-only cache still compiles what it already holds, so it
# only gets a warning. See
# docs/leanings/2026-09-28-swift-cache-spelling-mismatch-isolated.md and
# docs/leanings/2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md.

canonicalize_clang_module_cache_path() {
    local requested="${CLANG_MODULE_CACHE_PATH:-}"
    local target="" physical=""
    [ -n "${requested}" ] || return 0
    # "./" keeps a relative operand away from CDPATH and from `cd -`.
    case "${requested}" in
        /*) target="${requested}" ;;
        *) target="./${requested}" ;;
    esac
    if ! /bin/mkdir -p -- "${target}" 2>/dev/null ||
        ! physical="$(CDPATH='' cd -P -- "${target}" 2>/dev/null && /bin/pwd -P)" ||
        [ -z "${physical}" ]; then
        echo "❌ CLANG_MODULE_CACHE_PATH is not a usable directory: ${requested}" >&2
        return 1
    fi
    if [ ! -w "${physical}" ]; then
        if [ -z "$(/bin/ls -A -- "${physical}" 2>/dev/null)" ]; then
            echo "❌ CLANG_MODULE_CACHE_PATH is an empty directory that is not writable: ${requested}" >&2
            return 1
        fi
        echo "⚠️  CLANG_MODULE_CACHE_PATH is not writable: ${requested}. A module missing from it will fail to build, reported as \"this SDK is not supported by the compiler\"." >&2
    fi
    if [ "${physical}" != "${requested}" ]; then
        echo "ℹ️  Using the physical Swift module cache path ${physical} (CLANG_MODULE_CACHE_PATH was ${requested})."
    fi
    export CLANG_MODULE_CACHE_PATH="${physical}"
}
