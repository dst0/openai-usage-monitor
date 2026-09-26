use super::unreadable_lines;

#[test]
fn values_left_open_at_line_end_are_unreadable() {
    for (text, line) in [
        ("a: \"x\nb: y\"\n", 1),
        ("a: 'x\n", 1),
        // An escaped quote does not close the scalar.
        ("a: 'it''s\n", 1),
        ("a: \"x\\\"\n", 1),
        // A trailing backslash escapes the line break.
        ("a: \"x\\\n", 1),
        ("a: [x,\n  y]\n", 1),
        ("a: {b: [1,\n", 1),
        // A comment ends the line, not the flow collection, even when the
        // comment text holds the closing bracket.
        ("a: [x # note\n  ]\n", 1),
        ("a: [x, # y]\n  z]\n", 1),
        ("- \"x\n", 1),
        ("- [x,\n", 1),
        ("\"key: x\n", 1),
        ("\"k\": \"v\n", 1),
        ("run: echo hi\nb: \"x\n", 2),
    ] {
        assert_eq!(unreadable_lines(text), vec![line], "{text:?}");
    }
}

#[test]
fn explicit_and_merge_keys_are_unreadable() {
    // libyaml gives the job an `if:` the required-job rule cannot see. The
    // `: value` line has no key `entry` can read either.
    assert_eq!(
        unreadable_lines("job:\n  ? if\n  : ${{ false }}\n"),
        vec![2, 3]
    );
    assert_eq!(unreadable_lines("a:\n  ?\n"), vec![2]);
    assert_eq!(unreadable_lines("- <<: *step\n  with: {}\n"), vec![1]);
    assert_eq!(unreadable_lines("a:\n  <<: [*x, *y]\n"), vec![2]);
}

/// Regression (critic c01-c09, j04-j08): anchors, aliases, tags, escapes,
/// and YAML 1.1 line breaks change what libyaml reads while the line reader
/// still sees ordinary keys, for example a checkout without its `with:`
/// block, a job `if:`, or `uses` spelled `"u\x73es"`.
#[test]
fn node_properties_escapes_and_foreign_line_breaks_are_unreadable() {
    for (text, line) in [
        ("a: &n \"x\n", 1),
        ("a: !!str 'x\n", 1),
        ("uses: &co actions/checkout@v4\n", 1),
        ("uses: !!str actions/checkout@v4\n", 1),
        ("x:\n  *u : actions/checkout@v4\n", 2),
        ("  &c if: ${{ false }}\n", 1),
        ("  !!str if: ${{ false }}\n", 1),
        ("- *step\n", 1),
        ("- &step\n  run: x\n", 1),
        // `!` starts a tag, which is why GitHub needs `${{ !cancelled() }}`.
        ("if: !cancelled()\n", 1),
        ("\"u\\x73es\": actions/checkout@v4\n", 1),
        ("\"i\\x66\": ${{ false }}\n", 1),
        ("uses: \"actions/ch\\x65ckout@v4\" # v4.4.0\n", 1),
        // `\/` spells `/`, so `actions\/checkout` is `actions/checkout`.
        ("uses: \"actions\\/checkout@v4\"\n", 1),
        ("run: \"echo a\\tb\"\n", 1),
        ("a: \"x\\\\\\x65\"\n", 1),
        ("- \"\\x21main\"\n", 1),
        ("a: 1\rif: x\n", 1),
        ("a: 1\u{85}if: x\n", 1),
        ("a: 1\u{2028}if: x\n", 1),
        ("a: 1\u{2029}if: x\n", 1),
        // A break inside a comment or block scalar ends it for libyaml.
        ("# note\u{2028}if: x\n", 1),
        ("run: |\n  echo\rif: x\n", 2),
    ] {
        assert_eq!(unreadable_lines(text), vec![line], "{text:?}");
    }
    // An anchored block scalar is rejected once; its content is still skipped.
    assert_eq!(
        unreadable_lines("run: &s |\n  echo \"x\n  y: [1,\n"),
        vec![1]
    );
}

/// Regression (PR #18 critic P0-1, P1-1): libyaml reads a deeper line after
/// a one-line plain value as more of that value, and `key : value` as a key,
/// so the reader saw a bare `-D warnings` gate and no job `if:`.
#[test]
fn continued_values_and_unparsed_keys_are_unreadable() {
    for (text, line) in [
        ("run: cargo clippy -- -D warnings\n  -A clippy::all\n", 2),
        ("if: success()\n  == false\n", 2),
        ("- run: x\n    y\n", 2),
        ("- main\n    - dev\n", 2),
        ("a: 'x'\n  b: 1\n", 2),
        ("a: [x]\n  b\n", 2),
        ("job:\n  if : false\n", 2),
        ("job:\n  if\t: false\n", 2),
        ("continue-on-error : true\n", 1),
        ("- if : failure()\n", 1),
        ("a:b: c\n", 1),
        ("- echo a: b\n", 1),
    ] {
        assert_eq!(unreadable_lines(text), vec![line], "{text:?}");
    }
}

/// Regression (PR #18 critic P0-2): `str::trim` removes U+00A0, so the reader
/// saw `-D warnings` where libyaml passes `warnings\u{a0}` to Clippy.
#[test]
fn whitespace_other_than_space_and_tab_is_unreadable() {
    for text in [
        "run: cargo clippy -- -D warnings\u{a0}\n",
        "a: b\u{3000}\n",
        "a:\u{2003}b\n",
        "a: b\x0b\n",
        "a: b\x0c\n",
        "# note\u{a0}\n",
    ] {
        assert_eq!(unreadable_lines(text), vec![1], "{text:?}");
    }
}

#[test]
fn single_line_values_and_block_scalars_are_readable() {
    for text in [
        "steps:\n  -\n    run: x\n",
        "- name: x\n  run: y\n",
        "jobs: # note\n  a: 1\n",
        "a: 1\n  # a deeper comment\nb: 2\n",
        "a:\tb\n",
        "a: 1\r\nb: 2\r\n",
        "run: echo a && b || c\n",
        "if: github.ref != 'refs/heads/main'\n",
        "if: ${{ !cancelled() }}\n",
        "cron: '*/5 * * * *'\n",
        "x: a*b&c!d\n",
        "a: 'C:\\path'\n",
        // `\"` and `\\` cannot spell a key or a compared name.
        "description: \"JSON such as '{\\\"DEBUG\\\":\\\"1\\\"}'\"\n",
        "path: \"C:\\\\tools\"\n",
        "run: |\n  echo \"a\\tb\" && x & y\n  *not: an alias\n",
        "branches: [ main, 'release/**' ]\n",
        "x: [a, 'b]', \"c,d\"]\n",
        "a: \"it's # not a comment\"\n",
        "a: 'it''s'\n",
        "a: \"x\" # note\n",
        "a: {b: [1, 2], c: 'd'}\n",
        // Quotes and brackets inside plain scalars are literal text.
        "run: echo it's fine\n",
        "run: echo \"unbalanced\n",
        "key: ${{ hashFiles('a') }}-[x\n",
        "name: 🦀 Rust [core] \"tests\"\n",
        "url: http://x/y\n",
        "- main\n",
        "- 'release/**' # comment\n",
        "- main # note: \"x\n",
        "- |\n  echo 'x\n",
        "run: |\n  echo \"open\n  x: [1,\n  ? key\n  <<: *a\n\n  # comment\nnext: 1\n",
        "script: |2 # indented\n   echo \"x\n",
        "steps:\n  - run: >-\n      echo 'x\n    name: y\n",
    ] {
        assert_eq!(unreadable_lines(text), Vec::<usize>::new(), "{text:?}");
    }
}

#[test]
fn block_scalars_end_at_their_key_column() {
    assert_eq!(
        unreadable_lines("run: |\n  echo \"x\nnext: \"open\n"),
        vec![3]
    );
    // The scalar belongs to `run`, whose key column is past the list marker.
    assert_eq!(
        unreadable_lines("steps:\n  - run: |\n      echo 'x\n    with: 'open\n"),
        vec![4]
    );
    // A quoted `|` is a plain string, not a block scalar header.
    assert_eq!(unreadable_lines("a: '|'\n  b: \"open\n"), vec![2]);
}

/// The critic's full-workflow cases fail the whole-workflow scan.
#[test]
fn critic_workflows_fail_the_workflow_scan() {
    use crate::fixtures::{with, SHA};
    let uses = format!("        uses: actions/checkout@{SHA} # v4.4.0\n");
    let checkout = format!("{uses}        with:\n          persist-credentials: false\n");
    for text in [
        with(&checkout, &uses.replace("uses: ", "uses: &co ")),
        with(&checkout, &uses.replace("uses: ", "uses: !!str ")),
        with(&checkout, "        \"u\\x73es\": actions/checkout@v4\n"),
        with(
            "          persist-credentials: false\n",
            "          persist-credentials: false\n          fetch-depth: 1\r        env:\n",
        ),
        with(
            "    timeout-minutes: 30\n",
            "    timeout-minutes: 30\n    &c if: ${{ false }}\n",
        ),
        with(
            "    runs-on: macos-14\n",
            "    runs-on: macos-14\u{85}    if: ${{ false }}\n",
        ),
    ] {
        let v = crate::rules::workflow_violations(&text);
        assert!(
            v.iter().any(|m| m.contains("cannot read")),
            "{text:?}\n{v:?}"
        );
    }
}
