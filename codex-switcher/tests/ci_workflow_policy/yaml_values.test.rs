use super::{flow_items, list_value, scalar_item};

#[test]
fn scalar_items_are_plain_names_or_fully_quoted() {
    for (text, item) in [
        ("main", "main"),
        (" release/** ", "release/**"),
        ("pull_request", "pull_request"),
        ("'!main'", "!main"),
        ("\"a b\"", "a b"),
    ] {
        assert_eq!(scalar_item(text), Some(item), "{text:?}");
    }
    for text in [
        "",
        "''",
        "'main",
        "\"a\"b\"",
        "!main",
        "*alias",
        "&anchor main",
        "a b",
        "a: b",
        "{}",
        "[main]",
        "a,b",
        // Escapes can spell `!main` without a leading `!`.
        "\"\\x21main\"",
        "\"\\u0021main\"",
    ] {
        assert_eq!(scalar_item(text), None, "{text:?}");
    }
}

#[test]
fn flow_items_read_one_line_sequences_only() {
    assert_eq!(
        flow_items("[ main, 'release/**', \"x\" ]"),
        Some(vec!["main", "release/**", "x"])
    );
    assert_eq!(flow_items("[]"), Some(Vec::new()));
    // A comma inside quotes, a trailing comma, nesting, or no brackets.
    for value in [
        "['a,b']",
        "[main, ]",
        "[[main]]",
        "main",
        "[main",
        "{main: x}",
    ] {
        assert_eq!(flow_items(value), None, "{value:?}");
    }
}

#[test]
fn list_values_read_flow_or_block_sequences() {
    fn read(text: &str) -> Option<Vec<&str>> {
        let lines: Vec<&str> = text.lines().collect();
        list_value(&lines, 0)
    }
    assert_eq!(read("b: [main] # c"), Some(vec!["main"]));
    assert_eq!(
        read("b: # c\n  - main # m\n  - 'r/**'\nnext: x"),
        Some(vec!["main", "r/**"])
    );
    // A compact sequence at the key's own indentation.
    assert_eq!(
        read("b:\n- main\n- dev\nnext: x"),
        Some(vec!["main", "dev"])
    );
    // A quoted string, a lone scalar, a null, mixed or nested items.
    for text in [
        "b: \"[main]\"",
        "b: main",
        "b:",
        "b:\nnext: x",
        "b:\n  - main\n  x: y",
        "b:\n  - main\n    - dev",
        "b:\n  - key: main",
    ] {
        assert_eq!(read(text), None, "{text:?}");
    }
}
