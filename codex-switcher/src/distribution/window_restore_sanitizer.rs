pub fn sanitize_operation_id(value: &str) -> String {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
    {
        return "op_redacted".into();
    }
    value.to_string()
}

pub fn sanitize_text(value: &str) -> String {
    let mut output = Vec::new();
    for token in value.split_whitespace() {
        let clean = if token.contains('@') && token.contains('.') {
            "[redacted-email]"
        } else if token.contains('/') || token.contains('\\') || token.starts_with('~') {
            "[redacted-path]"
        } else if contains_uuid(token) {
            "[redacted-id]"
        } else if token.len() > 160 {
            "[redacted-detail]"
        } else {
            token
        };
        output.push(clean);
    }
    output.join(" ")
}

fn contains_uuid(value: &str) -> bool {
    value
        .split(|character: char| character != '-' && !character.is_ascii_hexdigit())
        .any(|candidate| {
            let segments: Vec<&str> = candidate.split('-').collect();
            segments.len() == 5
                && [8, 4, 4, 4, 12]
                    .iter()
                    .zip(segments.iter())
                    .all(|(expected, segment)| {
                        segment.len() == *expected
                            && segment
                                .chars()
                                .all(|character| character.is_ascii_hexdigit())
                    })
        })
}
