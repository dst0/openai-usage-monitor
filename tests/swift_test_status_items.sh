#!/bin/bash
# Static check: Swift tests remove every status item they create.
#
# A status item made with statusItem(withLength:) sits in the developer's real
# menu bar until removeStatusItem(_:) takes it out or the test binary exits, and
# nothing can list status items, so no runtime check can find one a test left
# behind. Every tests/*.swift file, at any depth, must therefore remove as many
# status items as it creates. Every call counts, several on one line too, and
# any receiver counts, so an alias of NSStatusBar.system is covered. A creation
# is statusItem( followed by withLength, even on the next line. A removal counts
# only as a direct member call, .removeStatusItem(item); any other use of that
# name (a declaration or wrapper, a selector, a method reference) and any
# reference to statusItem(withLength:) fails the check, because a wrapper called
# as self.removeStatusItem(item) need not remove anything. Text in comments and
# literals does not count: STRIP_SWIFT removes // and nested /* */ comments,
# plain, multi-line, and raw strings, and extended regex literals (#/.../#)
# first, and keeps the code of string interpolations. The check counts calls
# and cannot pair them: review still checks that each removal follows its
# item's last use.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && /bin/pwd -P)"
TEMP_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/codex-swift-status-item-lint.XXXXXX")"
trap '/bin/rm -rf -- "${TEMP_ROOT}"' EXIT

# STRIP_SWIFT: a perl program that prints Swift source from standard input with
# comments, string literals, and extended regex literals removed and line breaks
# kept. A regex literal ends at the first / and its #s that no backslash
# escapes, as Swift lexes it. The program holds no single quote so it can live
# in this shell string: macOS bash 3.2 writes here-documents outside TMPDIR
# (docs/leanings/2026-09-28-bash-heredocs-ignore-tmpdir-under-a-write-sandbox.md).
STRIP_SWIFT='
use strict; use warnings;
local $/; my $s = <STDIN>; $s = "" unless defined $s;
my $n = length $s; my $i = 0;
sub code {
  my ($inner) = @_; my $out = ""; my $depth = 0;
  while ($i < $n) {
    my $c = substr($s, $i, 1); my $two = substr($s, $i, 2);
    if ($two eq "//") { my $j = index($s, "\n", $i); $i = $j < 0 ? $n : $j; next; }
    if ($two eq "/*") {
      my $level = 1; $i += 2;
      while ($i < $n && $level > 0) {
        my $t = substr($s, $i, 2);
        if ($t eq "/*") { $level++; $i += 2; }
        elsif ($t eq "*/") { $level--; $i += 2; }
        else { $out .= "\n" if substr($s, $i, 1) eq "\n"; $i++; }
      }
      next;
    }
    if ($c eq "\"" || $c eq "#") {
      my $k = $i; my $hashes = 0;
      while ($k < $n && substr($s, $k, 1) eq "#") { $hashes++; $k++; }
      if ($k < $n && substr($s, $k, 1) eq "\"") { $i = $k; $out .= literal($hashes); next; }
      if ($hashes > 0 && $k < $n && substr($s, $k, 1) eq "/") { $i = $k + 1; $out .= regex($hashes); next; }
    }
    if ($inner) {
      if ($c eq "(") { $depth++; }
      elsif ($c eq ")") { return $out if $depth == 0; $depth--; }
    }
    $out .= $c; $i++;
  }
  return $out;
}
sub literal {
  my ($hashes) = @_; my $fence = "#" x $hashes;
  my $multi = substr($s, $i, 3) eq "\"\"\""; $i += $multi ? 3 : 1;
  my $close = ($multi ? "\"\"\"" : "\"") . $fence; my $escape = "\\" . $fence;
  my $out = "\"\"";
  while ($i < $n) {
    if (substr($s, $i, length $close) eq $close) { $i += length $close; return $out; }
    if (substr($s, $i, length $escape) eq $escape) {
      $i += length $escape;
      if (substr($s, $i, 1) eq "(") { $i++; $out .= " " . code(1) . " "; $i++; } else { $i++; }
      next;
    }
    $out .= "\n" if substr($s, $i, 1) eq "\n"; $i++;
  }
  return $out;
}
sub regex {
  my ($hashes) = @_; my $close = "/" . ("#" x $hashes); my $out = "\"\"";
  while ($i < $n) {
    if (substr($s, $i, length $close) eq $close) { $i += length $close; return $out; }
    $i++ if substr($s, $i, 1) eq "\\";
    $out .= "\n" if substr($s, $i, 1) eq "\n"; $i++;
  }
  return $out;
}
print code(0);
'

# COUNT_CALLS: a perl program that reads STRIP_SWIFT output and prints
# "created removed misused": misused counts each removeStatusItem that is not a
# .removeStatusItem(item) call and each statusItem(withLength:) reference.
COUNT_CALLS='
use strict; use warnings;
local $/; my $c = <STDIN>; $c = "" unless defined $c;
my $created = () = $c =~ /statusItem\s*\(\s*withLength\b/g;
my $removed = () = $c =~ /\.removeStatusItem[ \t]*\((?!\s*_\s*:\s*\))/g;
my $named = () = $c =~ /\bremoveStatusItem\b/g;
my $references = () = $c =~ /statusItem\s*\(\s*withLength\s*:\s*\)/g;
print $created, " ", $removed, " ", $named - $removed + $references, "\n";
'

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# scan FILE -> "created removed misused" for FILE's code, outside comments and
# literals. Every match counts, so two calls on one line count twice. Fails if
# either program fails.
scan() {
    /usr/bin/perl -T -e "${STRIP_SWIFT}" < "$1" | /usr/bin/perl -T -e "${COUNT_CALLS}"
}

# check_tree ROOT -> prints each violation; returns 1 if there is any. A listing
# error fails the check rather than skipping part of the tree.
check_tree() {
    local root="$1" violations="" files file rel counts created removed misused status=0
    local counts_re='^([0-9]+) ([0-9]+) ([0-9]+)$'
    [ -d "${root}/tests" ] || { printf 'tests: no such directory under %s\n' "${root}"; return 1; }
    files="$(/usr/bin/find "${root}/tests" -type f -name '*.swift')" || status=$?
    if [ "${status}" -ne 0 ]; then
        printf 'tests: cannot list every Swift test (find exited %s)\n' "${status}"
        return 1
    fi
    while IFS= read -r file; do
        [ -n "${file}" ] || continue
        rel="${file#"${root}"/}"
        if [ ! -r "${file}" ]; then
            violations+="${rel}: cannot be read"$'\n'
            continue
        fi
        if ! counts="$(scan "${file}")" || [[ ! "${counts}" =~ ${counts_re} ]]; then
            violations+="${rel}: cannot be scanned"$'\n'
            continue
        fi
        created="${BASH_REMATCH[1]}" removed="${BASH_REMATCH[2]}" misused="${BASH_REMATCH[3]}"
        [ "${misused}" -eq 0 ] \
            || violations+="${rel} has ${misused} uses of removeStatusItem or statusItem(withLength:) that are not direct calls; call both directly"$'\n'
        [ "${created}" -eq "${removed}" ] \
            || violations+="${rel} creates ${created} status items and removes ${removed}; remove each after its last use"$'\n'
    done < <(printf '%s\n' "${files}" | /usr/bin/sort)
    if [ -n "${violations}" ]; then
        printf '%s' "${violations}"
        return 1
    fi
}

# --- The checker rejects each kind of violation --------------------------------
make_fixture() {
    local dir="${TEMP_ROOT}/$1"
    /bin/mkdir -p "${dir}/tests"
    printf '%s\n' \
        '  // A comment may name NSStatusBar.system.statusItem(withLength: 1).' \
        '  let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)' \
        '  delegate.statusItem = item' \
        '  NSStatusBar.system.removeStatusItem(item)' \
        '  let doc = "NSStatusBar.system.statusItem(withLength: 3) is made in setUp"' \
        '  let extra = NSStatusBar.system.statusItem(withLength: 4); print("\(extra.title ?? "none") // https://x"); NSStatusBar.system.removeStatusItem(extra)' \
        > "${dir}/tests/AppDelegateTests.swift"
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

expect_accepted() {
    # expect_accepted NAME LINE: the fixture plus LINE in the default file is accepted
    local dir output
    dir="$(make_fixture "$1")"
    printf '%s\n' "$2" >> "${dir}/tests/AppDelegateTests.swift"
    output="$(check_tree "${dir}")" || fail "$1: the checker rejected: $2
${output}"
}

clean="$(make_fixture clean)"
output="$(check_tree "${clean}")" || fail "clean fixture was rejected: ${output}"
# A call split over lines counts once; similar names and other regex literals are not status-item calls.
expect_accepted split-calls \
    '  let split = NSStatusBar.system.statusItem('$'\n''    withLength: 8)'$'\n''  NSStatusBar.system'$'\n''    .removeStatusItem(split)'
expect_accepted similar-names '  cache.removeStatusItemFromCache(id); let words = #/[a-z]+/#; let label = statusItemTitle(id)'

T=tests/AppDelegateTests.swift
expect_violation kept "${T}" \
    '  appDelegate.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)' \
    "${T} creates 3 status items and removes 2"
expect_violation aliased "${T}" '  let other = bar.statusItem(withLength: 24)' "${T} creates 3 status items and removes 2"
expect_violation spaced "${T}" '  let other = NSStatusBar.system.statusItem (withLength: 24)' \
    "${T} creates 3 status items and removes 2"
expect_violation split-call "${T}" '  let other = NSStatusBar.system.statusItem('$'\n''    withLength: 24)' \
    "${T} creates 3 status items and removes 2"
expect_violation same-line "${T}" \
    '  let a = NSStatusBar.system.statusItem(withLength: 1); let b = NSStatusBar.system.statusItem(withLength: 2); NSStatusBar.system.removeStatusItem(a)' \
    "${T} creates 4 status items and removes 3"
# Removal-shaped text in strings and comments is not a removal.
LEAK='  let kept = NSStatusBar.system.statusItem(withLength: 5)'
expect_violation string-removal "${T}" "${LEAK}; let note = \"call removeStatusItem(kept)\"" \
    "${T} creates 3 status items and removes 2"
expect_violation trailing-comment-removal "${T}" "${LEAK} // NSStatusBar.system.removeStatusItem(kept)" \
    "${T} creates 3 status items and removes 2"
expect_violation block-comment-removal "${T}" "${LEAK}"$'\n''  /* later:'$'\n''  NSStatusBar.system.removeStatusItem(kept) */' \
    "${T} creates 3 status items and removes 2"
expect_violation nested-comment-removal "${T}" \
    "${LEAK}; /* outer /* inner */ NSStatusBar.system.removeStatusItem(kept) */" \
    "${T} creates 3 status items and removes 2"
expect_violation multiline-string-removal "${T}" \
    "${LEAK}"$'\n''  let doc2 = """'$'\n''  a " NSStatusBar.system.removeStatusItem(kept) "'$'\n''  """' \
    "${T} creates 3 status items and removes 2"
expect_violation raw-string-removal "${T}" \
    "${LEAK}; let raw = #\"one \" NSStatusBar.system.removeStatusItem(kept) \"#" \
    "${T} creates 3 status items and removes 2"
expect_violation interpolated-literal-removal "${T}" \
    "${LEAK}; let s = \"\\(label(\"NSStatusBar.system.removeStatusItem(kept)\"))\"" \
    "${T} creates 3 status items and removes 2"
# A removal counts only as a direct member call: not a declaration, selector,
# method reference, or text in an extended regex literal.
expect_violation declared-removal "${T}" "${LEAK}"$'\n''  func removeStatusItem(_ item: NSStatusItem) {}' \
    "${T} creates 3 status items and removes 2"
expect_violation selector-removal "${T}" "${LEAK}; let action = #selector(removeStatusItem(_:))" \
    "${T} creates 3 status items and removes 2"
expect_violation referenced-removal "${T}" "${LEAK}; let remove = NSStatusBar.system.removeStatusItem(_:)" \
    "${T} creates 3 status items and removes 2"
expect_violation regex-removal "${T}" "${LEAK}; let pattern = #/NSStatusBar.system.removeStatusItem(kept)/#" \
    "${T} creates 3 status items and removes 2"
expect_violation escaped-regex-removal "${T}" \
    "${LEAK}; let pattern = #/a\\/# NSStatusBar.system.removeStatusItem(kept) /#" \
    "${T} creates 3 status items and removes 2"
expect_violation multiline-regex-removal "${T}" \
    "${LEAK}"$'\n''  let pattern = #/'$'\n''  NSStatusBar.system.removeStatusItem(kept)'$'\n''  /#' \
    "${T} creates 3 status items and removes 2"
# A quote in a regex literal does not start a string that hides later code.
expect_violation regex-quote "${T}" \
    '  let quote = #/"/#; let other = NSStatusBar.system.statusItem(withLength: 7); let q = "x"' \
    "${T} creates 3 status items and removes 2"
# Any other use of removeStatusItem fails, even when the counts balance: a wrapper
# called through self need not remove anything.
expect_violation wrapper-removal "${T}" \
    "${LEAK}; func removeStatusItem(_ item: NSStatusItem) {}; self.removeStatusItem(kept)" \
    "${T} has 1 uses of removeStatusItem or statusItem(withLength:) that are not direct calls"
expect_violation unapplied-removal "${T}" '  let remove = NSStatusBar.system.removeStatusItem' \
    "${T} has 1 uses of removeStatusItem or statusItem(withLength:) that are not direct calls"
expect_violation referenced-creation "${T}" '  let make = NSStatusBar.system.statusItem(withLength:)' \
    "${T} has 1 uses of removeStatusItem or statusItem(withLength:) that are not direct calls"
# Code inside an interpolation is code: a status item made there counts.
expect_violation interpolated-creation "${T}" \
    '  let width = "\(NSStatusBar.system.statusItem(withLength: 6).length)"' \
    "${T} creates 3 status items and removes 2"
expect_violation extra-removal "${T}" '  NSStatusBar.system.removeStatusItem(item)' \
    "${T} creates 2 status items and removes 3"
expect_violation subdirectory tests/support/Helpers.swift \
    '  let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)' \
    'tests/support/Helpers.swift creates 1 status items and removes 0'
# Counts are per file: one file's extra removal must not hide another file's leak.
split_files="$(make_fixture split-files)"
printf '%s\n' '  let other = NSStatusBar.system.statusItem(withLength: 24)' >> "${split_files}/${T}"
printf '%s\n' '  NSStatusBar.system.removeStatusItem(other)' > "${split_files}/tests/OtherTests.swift"
if output="$(check_tree "${split_files}")"; then
    fail "split-files: the checker accepted a leak offset by another file's extra removal"
fi
printf '%s\n' "${output}" | /usr/bin/grep -qF "${T} creates 3 status items and removes 2" \
    || fail "split-files: unexpected report: ${output}"
if [ "$(/usr/bin/id -u)" -ne 0 ]; then
    hidden="$(make_fixture unlistable)"
    /bin/mkdir -p "${hidden}/tests/sub"
    printf '%s\n' '  let item = NSStatusBar.system.statusItem(withLength: 24)' > "${hidden}/tests/sub/Bad.swift"
    /bin/chmod 000 "${hidden}/tests/sub"
    if output="$(check_tree "${hidden}" 2>/dev/null)"; then
        /bin/chmod 700 "${hidden}/tests/sub"
        fail "unlistable: the checker accepted a tree it could not list"
    fi
    /bin/chmod 700 "${hidden}/tests/sub"
    printf '%s\n' "${output}" | /usr/bin/grep -qF 'tests: cannot list every Swift test' \
        || fail "unlistable: unexpected report: ${output}"
    unreadable="$(make_fixture unreadable)"
    /bin/chmod 000 "${unreadable}/${T}"
    if output="$(check_tree "${unreadable}")"; then
        fail "unreadable: the checker accepted an unreadable test file"
    fi
    printf '%s\n' "${output}" | /usr/bin/grep -qF "${T}: cannot be read" \
        || fail "unreadable: unexpected report: ${output}"
fi

# --- The repository follows the rule ------------------------------------------
output="$(check_tree "${PROJECT_DIR}")" || fail "Swift tests leave status items in the menu bar:
${output}"

echo "swift test status items: ok"
