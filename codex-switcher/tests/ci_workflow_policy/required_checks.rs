//! Jobs named by the required branch-protection contexts must always run and
//! report their real result on pull requests.

use crate::triggers::pull_request_trigger_violations;
use crate::workflow_jobs::{job_steps, jobs, Job};
use crate::yaml_lines::entry;
use crate::yaml_values::names;

/// Job keys that let a required check be skipped, renamed, or pass on failure.
const REQUIRED_JOB_FORBIDDEN_KEYS: [&str; 3] = ["if", "strategy", "continue-on-error"];

/// Step conditions under which a step runs whenever its job can still pass,
/// so the condition cannot skip the step that does the checking.
const ALLOWED_STEP_CONDITIONS: [&str; 4] = [
    "always()",
    "success()",
    "${{ always() }}",
    "${{ success() }}",
];

/// Required branch-protection contexts must each name exactly one
/// unconditional, unfiltered job of a workflow that runs for every pull
/// request into `protected_branch`.
pub fn required_check_violations(
    text: &str,
    contexts: &[String],
    protected_branch: &str,
) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let jobs = jobs(text);
    let is_required = |job: &Job| {
        job.prop("name")
            .is_some_and(|n| contexts.iter().any(|c| c == n))
    };
    let required: Vec<&Job> = jobs.iter().filter(|j| is_required(j)).collect();
    let mut out = Vec::new();
    for context in contexts {
        let named: Vec<&Job> = jobs
            .iter()
            .filter(|j| j.prop("name") == Some(context.as_str()))
            .collect();
        match named[..] {
            [] => out.push(format!(
                "required check `{context}` has no job with that name"
            )),
            [job] => out.extend(required_job_violations(&lines, job, &required)),
            _ => out.push(format!(
                "required check `{context}` names {} jobs; exactly one may report it",
                named.len()
            )),
        }
    }
    out.extend(pull_request_trigger_violations(text, protected_branch));
    out
}

/// A job skipped by its own condition, by a skipped job it needs, or by
/// step conditions reports success without doing its work.
fn required_job_violations(lines: &[&str], job: &Job, required: &[&Job]) -> Vec<String> {
    let id = &job.id;
    let mut out: Vec<String> = REQUIRED_JOB_FORBIDDEN_KEYS
        .iter()
        .filter(|k| job.prop(k).is_some())
        .map(|key| format!("required job `{id}` must not set `{key}`"))
        .collect();
    if let Some(at) = job.prop_line("needs") {
        let needs_required = names(lines, at)
            .is_some_and(|needs| needs.iter().all(|n| required.iter().any(|r| r.id == *n)));
        if !needs_required {
            out.push(format!("required job `{id}` may only need required jobs"));
        }
    }
    let Some(steps) = job_steps(lines, job) else {
        out.push(format!(
            "required job `{id}` steps are not a block list the policy can read"
        ));
        return out;
    };
    for i in steps.into_iter().flatten() {
        match entry(lines[i]) {
            Some(e) if e.key == "continue-on-error" => out.push(format!(
                "required job `{id}` step at line {} must not set `continue-on-error`",
                i + 1
            )),
            Some(e) if e.key == "if" && !ALLOWED_STEP_CONDITIONS.contains(&e.value) => {
                out.push(format!(
                    "required job `{id}` step at line {} must not be skipped by `if: {}`",
                    i + 1,
                    e.value
                ))
            }
            _ => {}
        }
    }
    out
}

/// Jobs outside the required-check workflow must not use a required context
/// as their name, or two jobs would report the same check.
pub fn reused_context_violations(text: &str, contexts: &[String]) -> Vec<String> {
    jobs(text)
        .into_iter()
        .filter_map(|job| {
            let name = job
                .prop("name")
                .filter(|n| contexts.iter().any(|c| c == n))?;
            Some(format!(
                "job `{}` reuses required check name `{name}`",
                job.id
            ))
        })
        .collect()
}

#[cfg(test)]
#[path = "required_checks.test.rs"]
mod tests;
