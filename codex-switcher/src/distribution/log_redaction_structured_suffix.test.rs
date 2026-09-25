use super::{quoted_suffix_is_structural, SuffixValidation};
use std::collections::HashSet;

#[test]
fn whitespace_tail_does_not_fill_boundary_cache() {
    let input = " ".repeat(1_048_576);
    let mut suffix = SuffixValidation {
        trusted: HashSet::new(),
        remaining: input.len().saturating_mul(4),
    };
    assert!(quoted_suffix_is_structural(&input, 0, &mut suffix));
    assert!(
        suffix.trusted.len() <= 2,
        "cached {} offsets",
        suffix.trusted.len()
    );
}
