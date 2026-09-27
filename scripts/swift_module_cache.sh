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
# compile steps. An unset or empty variable is left alone: Swift then uses its
# own default cache. See
# docs/leanings/2026-09-28-swift-cache-spelling-mismatch-isolated.md.

canonicalize_clang_module_cache_path() {
    local requested="${CLANG_MODULE_CACHE_PATH:-}"
    local physical=""
    [ -n "${requested}" ] || return 0
    if ! /bin/mkdir -p -- "${requested}" 2>/dev/null ||
        ! physical="$(CDPATH='' cd -P -- "${requested}" 2>/dev/null && /bin/pwd -P)" ||
        [ -z "${physical}" ]; then
        echo "❌ CLANG_MODULE_CACHE_PATH is not a usable directory: ${requested}" >&2
        return 1
    fi
    if [ "${physical}" != "${requested}" ]; then
        echo "ℹ️  Using the physical Swift module cache path ${physical} (CLANG_MODULE_CACHE_PATH was ${requested})."
    fi
    export CLANG_MODULE_CACHE_PATH="${physical}"
}
