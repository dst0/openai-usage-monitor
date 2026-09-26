//! `actions/checkout` steps must not leave the job token in `.git/config`.

use crate::yaml_lines::{entry, indent, is_content};

/// Every `actions/checkout` step must set `persist-credentials: false`.
pub fn checkout_credential_violations(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(e) =
            entry(line).filter(|e| e.key == "uses" && e.value.starts_with("actions/checkout@"))
        else {
            continue;
        };
        // The step's `- ` marker sits on this line or two columns to the left.
        let step_indent = if e.list_item {
            indent(line)
        } else {
            indent(line).saturating_sub(2)
        };
        let disabled = lines[i + 1..]
            .iter()
            .filter(|l| is_content(l))
            .take_while(|l| indent(l) > step_indent)
            .any(|l| {
                entry(l).is_some_and(|e| e.key == "persist-credentials" && e.value == "false")
            });
        if !disabled {
            out.push(format!(
                "checkout at line {} must set `persist-credentials: false`",
                i + 1
            ));
        }
    }
    out
}

#[cfg(test)]
#[path = "checkout.test.rs"]
mod tests;
