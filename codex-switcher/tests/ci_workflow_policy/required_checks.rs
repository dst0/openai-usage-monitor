//! Jobs named by the required branch-protection contexts must always run and
//! report their real result on pull requests.

use crate::triggers::pull_request_trigger_violations;
use crate::workflow_jobs::jobs;

/// Job keys that let a required check be skipped, renamed, or pass on failure.
const REQUIRED_JOB_FORBIDDEN_KEYS: [&str; 3] = ["if", "strategy", "continue-on-error"];

/// Required branch-protection contexts must be unconditional, unfiltered jobs
/// of a workflow that runs for every pull request into `protected_branch`.
pub fn required_check_violations(
    text: &str,
    contexts: &[String],
    protected_branch: &str,
) -> Vec<String> {
    let jobs = jobs(text);
    let mut out = Vec::new();
    for context in contexts {
        let Some(job) = jobs
            .iter()
            .find(|j| j.prop("name") == Some(context.as_str()))
        else {
            out.push(format!(
                "required check `{context}` has no job with that name"
            ));
            continue;
        };
        for key in REQUIRED_JOB_FORBIDDEN_KEYS
            .iter()
            .filter(|k| job.prop(k).is_some())
        {
            out.push(format!("required job `{}` must not set `{key}`", job.id));
        }
    }
    out.extend(pull_request_trigger_violations(text, protected_branch));
    out
}
