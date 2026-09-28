#!/bin/bash
# Sourced by scripts/install.sh and scripts/test_swift.sh before their first
# swiftc call.
#
# Implicitly built Clang modules (.pcm) record the absolute paths of the module
# files they import, spelled the way the module cache path was spelled when
# they were built. Reusing a warm cache through any other path fails: another
# spelling of the same directory (/tmp/x for /private/tmp/x, since /tmp is a
# symlink on macOS), or a copied or moved cache directory. The error depends
# on the toolchain and the source: for another spelling, Swift 6.4 reports
# "module '_DarwinFoundation1' is defined in both ..." (and crashed on some
# sources) and Swift 5.10 "PCH was compiled with module cache path ..."; for a
# copied or moved cache, Swift 6.4 reports "precompiled file ... was compiled
# with module cache path ...".
#
# So these scripts never build into CLANG_MODULE_CACHE_PATH itself, which
# other tools may already have filled through another spelling. They use a
# subdirectory of its physical path, named after a checksum of that physical
# path. Every spelling of the directory resolves to the same physical path and
# so to the same subdirectory, which only this function ever hands to the
# compiler. A copied or moved cache gets a new, empty subdirectory. A value
# that already is that subdirectory for its parent, such as one this function
# exported to a nested script, is used as is. Resolving
# once also pins a relative path to the caller's directory, so the installer's
# later `cd` calls cannot move the cache between compile steps. An unset or
# empty variable is left alone, as before.
#
# A cache the compiler cannot write fails as soon as a module is missing, which
# a warm cache hits on the first compile with other flags (these scripts use
# several), and Swift may then report "this SDK is not supported by the
# compiler". The subdirectory must therefore be writable and searchable.
# `[ -w ]` asks the kernel (access(2)), which also applies ACLs, read-only
# mounts, and sandbox rules, so no probe file is left in the user's cache. It
# must also be ours and closed to other writers, who could otherwise plant
# modules that get compiled into the installed app. See
# docs/leanings/2026-09-28-swift-cache-spelling-mismatch-isolated.md,
# docs/leanings/2026-09-28-swift-sdk-not-supported-was-unwritable-module-cache.md,
# and docs/leanings/2026-09-28-swift-cache-helper-owns-a-path-keyed-subdirectory.md.

# The scripts' subdirectory is this prefix plus the POSIX cksum of the physical
# path of CLANG_MODULE_CACHE_PATH.
SWIFT_MODULE_CACHE_SUBDIR_PREFIX="codex-monitor-swift-"

# swift_module_cache_subdir_name PHYSICAL_DIR -> prints the subdirectory name.
swift_module_cache_subdir_name() {
    local checksum
    checksum="$(/usr/bin/printf '%s' "$1" | /usr/bin/cksum)" || return 1
    checksum="${checksum%% *}"
    case "${checksum}" in
        '' | *[!0-9]*) return 1 ;;
    esac
    /usr/bin/printf '%s%s' "${SWIFT_MODULE_CACHE_SUBDIR_PREFIX}" "${checksum}"
}

canonicalize_clang_module_cache_path() {
    local requested="${CLANG_MODULE_CACHE_PATH:-}"
    local target="" physical="" parent="" own_name="" name="" cache="" owner_mode=""
    [ -n "${requested}" ] || return 0
    case "${requested}" in
        *$'\n'*)
            echo "❌ CLANG_MODULE_CACHE_PATH contains a newline; use a directory path without one." >&2
            return 1
            ;;
        '~'*)
            echo "❌ CLANG_MODULE_CACHE_PATH starts with '~', which the shell did not expand: ${requested}" >&2
            echo "   Use \"\${HOME}/...\" or another absolute path." >&2
            return 1
            ;;
        /*) target="${requested}" ;;
        # "./" keeps a relative operand away from CDPATH, `cd -`, and options.
        *) target="./${requested}" ;;
    esac
    if ! /bin/mkdir -p -- "${target}" 2>/dev/null ||
        ! physical="$(cd -P -- "${target}" 2>/dev/null && /bin/pwd -P)"; then
        echo "❌ CLANG_MODULE_CACHE_PATH is not a usable directory: ${requested}" >&2
        return 1
    fi
    # Command substitution drops a newline that ends the physical path (through
    # a symlink), which would name a different directory: require the result
    # to be the requested directory itself.
    if [ ! -d "${physical}" ] || [ ! "${physical}" -ef "${target}" ]; then
        echo "❌ CLANG_MODULE_CACHE_PATH did not resolve to itself: ${requested}" >&2
        return 1
    fi
    case "${physical}" in
        *$'\n'*)
            echo "❌ CLANG_MODULE_CACHE_PATH resolves to a path that contains a newline: ${requested}" >&2
            return 1
            ;;
    esac

    parent="${physical%/*}"
    if ! own_name="$(swift_module_cache_subdir_name "${parent:-/}")" ||
        ! name="$(swift_module_cache_subdir_name "${physical}")"; then
        echo "❌ Could not name the Swift module cache for ${physical}" >&2
        return 1
    fi
    if [ "${physical##*/}" = "${own_name}" ]; then
        cache="${physical}"
    else
        cache="${physical%/}/${name}"
    fi
    # A symlink here would let the subdirectory be reached through a second
    # spelling again.
    if [ -L "${cache}" ] || { [ -e "${cache}" ] && [ ! -d "${cache}" ]; }; then
        echo "❌ ${cache} is not a plain directory; remove it or choose another CLANG_MODULE_CACHE_PATH." >&2
        return 1
    fi
    if [ ! -d "${cache}" ] && ! /bin/mkdir -m 755 -- "${cache}" 2>/dev/null; then
        echo "❌ The Swift module cache ${cache} cannot be created (CLANG_MODULE_CACHE_PATH=${requested})." >&2
        echo "   Choose a directory you can write." >&2
        return 1
    fi
    # Owner uid and permission bits; group or other write bits are refused.
    owner_mode="$(/usr/bin/stat -f '%u %Lp' "${cache}" 2>/dev/null)" || owner_mode="unknown"
    case "${owner_mode}" in
        "${EUID} "[0-7][0-7][0-7] | "${EUID} "[0-7][0-7][0-7][0-7])
            [ $((8#${owner_mode##* } & 8#022)) -eq 0 ]
            ;;
        *) false ;;
    esac || {
        echo "❌ ${cache} must be owned by you and not writable by group or others (owner and mode: ${owner_mode})." >&2
        return 1
    }
    if [ ! -w "${cache}" ] || [ ! -x "${cache}" ]; then
        echo "❌ The Swift module cache ${cache} cannot be written (CLANG_MODULE_CACHE_PATH=${requested})." >&2
        echo "   Swift would fail on the first module it has to build, possibly reported as \"this SDK is not supported by the compiler\"." >&2
        echo "   Choose a directory you can write." >&2
        return 1
    fi
    echo "ℹ️  Swift module cache: ${cache}, used only by these scripts (CLANG_MODULE_CACHE_PATH=${requested})." >&2
    export CLANG_MODULE_CACHE_PATH="${cache}"
}
