//! CI baseline rules from AGENTS.md. Each function returns human-readable
//! violations; an empty vector means the input complies.

use crate::checkout::checkout_credential_violations;
use crate::locked_cargo::unlocked_cargo_violations;
use crate::workflow_jobs::jobs;
use crate::yaml_limits::unreadable_line_violations;
use crate::yaml_lines::{block_after, entry, indent, is_content, top_level_block};

/// Upper bound for any job's `timeout-minutes`; a larger value is not a guard.
pub const MAX_JOB_TIMEOUT_MINUTES: u32 = 60;
const FLOATING_CHANNELS: [&str; 3] = ["stable", "beta", "nightly"];
/// Toolchain selectors that bypass `rust-toolchain.toml`.
const TOOLCHAIN_OVERRIDES: [&str; 5] = [
    "RUSTUP_TOOLCHAIN",
    "rustup override",
    "rustup run",
    "rustup default",
    "cargo +",
];
const MASKED_FAILURES: [&str; 3] = ["|| true", "|| :", "set +e"];

fn is_full_sha(rev: &str) -> bool {
    rev.len() == 40
        && rev
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `vX.Y.Z` with numeric parts.
fn is_release_tag(comment: &str) -> bool {
    let parts: Vec<&str> = comment.strip_prefix('v').unwrap_or("").split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn mentions_uses_key(line: &str) -> bool {
    line.match_indices("uses").any(|(i, _)| {
        let after = line[i + 4..].trim_start_matches(['"', '\'']).trim_start();
        after.starts_with(':')
    })
}

pub fn action_pin_violations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines().filter(|l| is_content(l)) {
        let Some(e) = entry(line).filter(|e| e.key == "uses") else {
            if mentions_uses_key(line) {
                out.push(format!(
                    "unparsed `uses:` line (use block style): `{}`",
                    line.trim()
                ));
            }
            continue;
        };
        let reference = e.value;
        if reference.starts_with("./") {
            out.push(format!(
                "local `{reference}` is outside the policy scan; extend ci_workflow_policy first"
            ));
            continue;
        }
        let pinned = match reference.split_once('@') {
            Some((_, rev)) if reference.starts_with("docker://") => rev
                .strip_prefix("sha256:")
                .is_some_and(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())),
            Some((_, rev)) => is_full_sha(rev),
            None => false,
        };
        if !pinned {
            out.push(format!(
                "`{reference}` is not pinned to a full commit SHA/digest"
            ));
        } else if !reference.starts_with("docker://") && !is_release_tag(e.comment) {
            out.push(format!("`{reference}` lacks a `# vX.Y.Z` release comment"));
        }
    }
    out
}

pub fn permission_violations(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut top_level_contents_read = false;
    for (i, line) in lines.iter().enumerate() {
        let Some(e) = entry(line).filter(|e| e.key == "permissions") else {
            continue;
        };
        if !e.value.is_empty() && e.value != "{}" {
            out.push(format!(
                "`permissions: {}` must list scopes explicitly",
                e.value
            ));
            continue;
        }
        for scope_line in block_after(&lines, i) {
            let (scope, access) =
                entry(scope_line).map_or((scope_line.trim(), ""), |s| (s.key, s.value));
            if access != "read" && access != "none" {
                out.push(format!("permission `{scope}: {access}` exceeds read-only"));
            }
            top_level_contents_read |= indent(line) == 0 && scope == "contents" && access == "read";
        }
    }
    if !top_level_contents_read {
        out.push("missing top-level `permissions: contents: read`".to_string());
    }
    out
}

pub fn timeout_violations(text: &str) -> Vec<String> {
    let jobs = jobs(text);
    if jobs.is_empty() {
        return vec!["workflow declares no jobs".to_string()];
    }
    let mut out = Vec::new();
    // GitHub rejects `timeout-minutes` on reusable-workflow (`uses:`) jobs.
    for job in jobs.iter().filter(|j| j.prop("uses").is_none()) {
        match job.prop("timeout-minutes").map(str::parse::<u32>) {
            Some(Ok(m)) if (1..=MAX_JOB_TIMEOUT_MINUTES).contains(&m) => {}
            Some(_) => out.push(format!(
                "job `{}` timeout-minutes must be 1..={MAX_JOB_TIMEOUT_MINUTES}",
                job.id
            )),
            None => out.push(format!("job `{}` has no timeout-minutes", job.id)),
        }
    }
    out
}

pub fn concurrency_violations(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(block) = top_level_block(&lines, "concurrency") else {
        return vec!["missing top-level `concurrency:`".to_string()];
    };
    let value = |key: &str| {
        block
            .iter()
            .filter_map(|l| entry(l))
            .find(|e| e.key == key)
            .map(|e| e.value)
    };
    let cancels = value("cancel-in-progress").is_some_and(|v| v == "true" || v.starts_with("${{"));
    if value("group").is_some_and(|g| !g.is_empty()) && cancels {
        Vec::new()
    } else {
        vec!["`concurrency:` needs a group and `cancel-in-progress`".to_string()]
    }
}

/// Any spelling of a floating channel or a per-command toolchain override.
pub fn floating_toolchain_violations(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| is_content(l))
        .map(|l| l.split(" #").next().unwrap_or(l))
        .filter(|l| {
            let floating = l
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| FLOATING_CHANNELS.contains(&w));
            floating || TOOLCHAIN_OVERRIDES.iter().any(|o| l.contains(o))
        })
        .map(|l| {
            format!(
                "Rust toolchain must come from rust-toolchain.toml: `{}`",
                l.trim()
            )
        })
        .collect()
}

pub fn masked_failure_violations(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| is_content(l) && MASKED_FAILURES.iter().any(|m| l.contains(m)))
        .map(|l| format!("command failure is masked: `{}`", l.trim()))
        .collect()
}

fn toml_setting<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        l.trim()
            .strip_prefix(key)?
            .trim()
            .strip_prefix('=')
            .map(str::trim)
    })
}

pub fn toolchain_channel(text: &str) -> &str {
    toml_setting(text, "channel")
        .unwrap_or("")
        .trim_matches('"')
}

pub fn toolchain_file_violations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let channel = toolchain_channel(text);
    if !is_release_tag(&format!("v{channel}")) {
        out.push(format!("channel `{channel}` is not an exact X.Y.Z release"));
    }
    for component in ["clippy", "rustfmt"] {
        if !toml_setting(text, "components")
            .is_some_and(|c| c.contains(&format!("\"{component}\"")))
        {
            out.push(format!("toolchain components must include {component}"));
        }
    }
    out
}

/// Branch in the protection script's `branches/<name>/protection` API path;
/// `None` unless exactly one literal branch name is protected.
pub fn protected_branch(script: &str) -> Option<&str> {
    let mut branches = script
        .split("/branches/")
        .skip(1)
        .filter_map(|rest| rest.split_once("/protection").map(|(branch, _)| branch));
    let branch = branches.next()?;
    let literal = !branch.is_empty()
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b));
    (literal && branches.next().is_none()).then_some(branch)
}

pub fn required_check_contexts(script: &str) -> Vec<String> {
    script
        .lines()
        .skip_while(|l| !l.contains("\"contexts\""))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with(']'))
        .map(|l| l.trim().trim_end_matches(',').trim_matches('"').to_string())
        .collect()
}

pub fn workflow_violations(text: &str) -> Vec<String> {
    [
        action_pin_violations(text),
        checkout_credential_violations(text),
        permission_violations(text),
        timeout_violations(text),
        concurrency_violations(text),
        floating_toolchain_violations(text),
        masked_failure_violations(text),
        unlocked_cargo_violations(text),
        unreadable_line_violations(text),
    ]
    .concat()
}
