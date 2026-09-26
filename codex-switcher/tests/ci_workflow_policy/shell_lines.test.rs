use super::ShellLines;

fn lines(text: &str) -> Vec<(usize, String)> {
    ShellLines::read(text)
        .lines
        .into_iter()
        .map(|(n, l)| (n, l.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect()
}

fn line(n: usize, text: &str) -> (usize, String) {
    (n, text.to_string())
}

/// Regression (review of this PR, F1): splitting at the first ` #` missed a
/// tab before `#` and cut quoted text, hiding the command after it.
#[test]
fn comments_start_outside_quotes_after_blank_or_at_line_start() {
    for (text, expected) in [
        ("cargo build\t# TODO\n", "cargo build"),
        ("cargo build # note\n", "cargo build"),
        (
            "echo \"Building #1\" && cargo build\n",
            "echo \"Building #1\" && cargo build",
        ),
        (
            "run: echo \"step #2\"; cargo test\n",
            "run: echo \"step #2\"; cargo test",
        ),
        ("echo 'a # b' && cargo test\n", "echo 'a # b' && cargo test"),
        // `#` inside a word, an expansion, or escaped is not a comment.
        ("echo a#b; cargo test\n", "echo a#b; cargo test"),
        ("echo ${#x} $#; cargo test\n", "echo ${#x} $#; cargo test"),
        (
            "echo \\# not a comment; cargo test\n",
            "echo \\# not a comment; cargo test",
        ),
        // Inside single quotes a backslash escapes nothing.
        ("echo 'a\\' # c\n", "echo 'a\\'"),
        // An escaped quote does not open or close a string.
        ("echo \\\" # c\n", "echo \\\""),
        ("echo \"a \\\" # b\" # c\n", "echo \"a \\\" # b\""),
    ] {
        assert_eq!(lines(text), [line(1, expected)], "{text:?}");
    }
    assert_eq!(lines("# only a comment\n  # indented\n\n"), []);
}

#[test]
fn continuations_join_until_a_comment_or_unescaped_line_end() {
    assert_eq!(
        lines("cargo test \\\n  --verbose \\\n  --locked\nnext\n"),
        [line(1, "cargo test --verbose --locked"), line(4, "next")]
    );
    // A comment ends the joined command, so bash never sees `--locked` as
    // part of it (review F4).
    assert_eq!(
        lines("cargo test \\\n  # c\n  --locked\n"),
        [line(1, "cargo test"), line(3, "--locked")]
    );
    // An escaped space is not a continuation.
    assert_eq!(
        lines("cargo test \\ \n--locked\n"),
        [line(1, "cargo test \\"), line(2, "--locked")]
    );
    // A continuation at the end of the text still yields its command.
    assert_eq!(lines("cargo build \\\n"), [line(1, "cargo build")]);
    // Inside single quotes a backslash is literal and continues nothing.
    let single = ShellLines::read("echo 'a \\\nb' && cargo test\n");
    assert_eq!(single.lines.len(), 2, "{single:?}");
    assert_eq!(single.open_quote_line, None);
}

/// Quoted programs (`osascript -e "…"`, `awk '…'`) span lines in the live
/// scripts; the quote state must follow them without joining the lines.
#[test]
fn quotes_carry_across_lines_without_joining_them() {
    let text = "awk '\n  # not a comment; cargo x\n' file # comment\ncargo test\n";
    let read = ShellLines::read(text);
    assert_eq!(read.open_quote_line, None);
    assert_eq!(
        lines(text),
        [
            line(1, "awk '"),
            line(2, "# not a comment; cargo x"),
            line(3, "' file"),
            line(4, "cargo test"),
        ]
    );
    // A later line's flag never joins a command on an earlier line.
    let text = "echo \"a\ncargo test\n\" --locked\n";
    assert_eq!(
        lines(text),
        [
            line(1, "echo \"a"),
            line(2, "cargo test"),
            line(3, "\" --locked")
        ]
    );
}

#[test]
fn an_unterminated_quote_reports_its_opening_line() {
    for (text, opened) in [
        ("a\necho \"never closed\ncargo test\n", 2),
        ("echo it's\n", 1),
        ("echo 'x' \"\n", 1),
        ("run: echo 'a\n  b'\n  c \"\n", 3),
    ] {
        assert_eq!(
            ShellLines::read(text).open_quote_line,
            Some(opened),
            "{text:?}"
        );
    }
    for text in ["echo 'x' \"y\"\n", "echo \"it's\"\n", "a: 'it''s'\n"] {
        assert_eq!(ShellLines::read(text).open_quote_line, None, "{text:?}");
    }
}

#[test]
fn yaml_name_labels_are_skipped_only_where_a_command_could_start() {
    assert_eq!(
        lines("      - name: Run cargo test\n        run: cargo test\n"),
        [line(2, "run: cargo test")]
    );
    // Inside a continued command or an open quote the line is shell text.
    assert_eq!(
        lines("cargo test \\\nname: x\n"),
        [line(1, "cargo test name: x")]
    );
    assert_eq!(
        lines("echo \"a\nname: cargo build\"\n"),
        [line(1, "echo \"a"), line(2, "name: cargo build\"")]
    );
}
