use super::{parse_key, quoted_value_end};

#[cfg(test)]
#[path = "log_redaction_structured_suffix.test.rs"]
mod tests;
use std::collections::HashSet;

pub(super) struct SuffixValidation {
    pub(super) trusted: HashSet<usize>,
    pub(super) remaining: usize,
}

pub(super) fn quoted_suffix_is_structural(
    input: &str,
    mut cursor: usize,
    suffix: &mut SuffixValidation,
) -> bool {
    let mut visited = vec![cursor];
    while let Some(character) = input[cursor..].chars().next() {
        if suffix.trusted.contains(&cursor) {
            suffix.trusted.extend(visited);
            return true;
        }
        let previous = cursor;
        if character.is_whitespace() {
            cursor += character.len_utf8();
            if suffix.remaining == 0 {
                return false;
            }
            suffix.remaining -= 1;
            continue;
        }
        if character == ',' {
            let next = cursor + character.len_utf8();
            let next = next + input[next..].len() - input[next..].trim_start().len();
            let Some((_, after_key, escaped_key)) = parse_key(input, next) else {
                return false;
            };
            if escaped_key {
                return false;
            }
            let mut value_start =
                after_key + input[after_key..].len() - input[after_key..].trim_start().len();
            let Some(delimiter) = input[value_start..].chars().next() else {
                return false;
            };
            if !matches!(delimiter, ':' | '=') {
                return false;
            }
            value_start += delimiter.len_utf8();
            value_start += input[value_start..].len() - input[value_start..].trim_start().len();
            let Some(first) = input[value_start..].chars().next() else {
                return false;
            };
            cursor = if matches!(first, '"' | '\'') {
                let start = value_start + first.len_utf8();
                let Some(end) = quoted_value_end(input, start, first) else {
                    return false;
                };
                end + first.len_utf8()
            } else {
                if matches!(first, ',' | '}' | ']' | ')' | '\\') {
                    return false;
                }
                let end = input[value_start..]
                    .char_indices()
                    .find(|(_, c)| c.is_whitespace() || matches!(c, ',' | '}' | ']' | ')'))
                    .map_or(input.len(), |(offset, _)| value_start + offset);
                if input[value_start..end]
                    .chars()
                    .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | '.'))
                {
                    return false;
                }
                end
            };
            if matches!(first, '"' | '\'') {
                visited.push(cursor);
            }
            let consumed = cursor - previous;
            if consumed > suffix.remaining {
                return false;
            }
            suffix.remaining -= consumed;
            continue;
        }
        if matches!(character, '}' | ']' | ')' | '"' | '\'') {
            // A forged closer must not release arbitrary text. Keep checking
            // the complete suffix; only a complete structural tail is safe.
            cursor += character.len_utf8();
            if suffix.remaining == 0 {
                return false;
            }
            suffix.remaining -= 1;
            continue;
        }
        return false;
    }
    suffix.trusted.extend(visited);
    true
}
