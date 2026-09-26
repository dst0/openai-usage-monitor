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
    // libyaml gives the job an `if:` the required-job rule cannot see.
    assert_eq!(
        unreadable_lines("job:\n  ? if\n  : ${{ false }}\n"),
        vec![2]
    );
    assert_eq!(unreadable_lines("a:\n  ?\n"), vec![2]);
    assert_eq!(unreadable_lines("- <<: *step\n  with: {}\n"), vec![1]);
    assert_eq!(unreadable_lines("a:\n  <<: [*x, *y]\n"), vec![2]);
}

#[test]
fn single_line_values_and_block_scalars_are_readable() {
    for text in [
        "branches: [ main, 'release/**' ]\n",
        "x: [a, 'b]', \"c,d\"]\n",
        "a: \"it's # not a comment\"\n",
        "a: 'it''s'\n",
        "a: \"x\\\"y\"\n",
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
