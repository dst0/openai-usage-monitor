use super::pull_request_trigger_violations;
use crate::fixtures::with;

/// The compliant fixture's triggers.
const ON_BLOCK: &str = "on:\n  pull_request:\n    branches: [ main ]\n";

fn violations_for(on: &str) -> Vec<String> {
    pull_request_trigger_violations(&with(ON_BLOCK, on), "main")
}

/// Regression (PR #13 review): a workflow with no `pull_request` trigger
/// reported no violation, so required checks could stop reporting on PRs
/// while the policy suite stayed green.
#[test]
fn a_pull_request_trigger_is_required() {
    for on in [
        "on: push\n",
        "on: [push]\n",
        "on: [ push, workflow_dispatch ]\n",
        "on:\n  - push\n",
        "on:\n  push:\n    branches: [ main ]\n",
        "on:\n  workflow_dispatch:\n",
        // Runs in the base branch's context, not as the PR's own check.
        "on:\n  pull_request_target:\n    branches: [ main ]\n",
        // Two `pull_request` keys: which one GitHub keeps is not the reader's call.
        "on:\n  pull_request:\n  pull_request:\n    branches: [ main ]\n",
        "",
    ] {
        let v = violations_for(on);
        assert_eq!(v.len(), 1, "{on:?}: {v:?}");
        assert!(
            v[0].contains("unconditional `pull_request`"),
            "{on:?}: {v:?}"
        );
    }
}

#[test]
fn unconditional_pull_request_triggers_comply() {
    for on in [
        "on: pull_request\n",
        "on: \"pull_request\" # every PR\n",
        "on: [push, pull_request]\n",
        "on: [ 'push', \"pull_request\" ]\n",
        "on:\n  - push\n  - pull_request\n",
        "on:\n  push:\n    branches: [ main ]\n  pull_request:\n",
        "on:\n  pull_request: # every PR\n  workflow_dispatch:\n",
        "\"on\":\n  pull_request:\n",
        "on:\n  pull_request:\n    branches:\n      - main\n      - 'release/**'\n",
        "on:\n  pull_request:\n    branches: [ 'main' ]\n    types: [opened, synchronize, reopened, ready_for_review]\n",
    ] {
        assert_eq!(violations_for(on), Vec::<String>::new(), "{on:?}");
    }
}

#[test]
fn filters_that_can_skip_pull_requests_are_rejected() {
    for (on, reason) in [
        (
            "on:\n  pull_request:\n    branches: [ develop ]\n",
            "must list `main`",
        ),
        (
            "on:\n  pull_request:\n    branches: [ main, '!main' ]\n",
            "must list `main`",
        ),
        (
            "on:\n  pull_request:\n    branches: main\n",
            "must list `main`",
        ),
        (
            "on:\n  pull_request:\n    branches: [ 'main,dev' ]\n",
            "must list `main`",
        ),
        (
            "on:\n  pull_request:\n    types: [ opened ]\n",
            "`synchronize`",
        ),
        (
            "on:\n  pull_request:\n    types:\n      - closed\n",
            "`synchronize`",
        ),
        (
            "on:\n  pull_request:\n    branches-ignore: [ develop ]\n",
            "`branches-ignore`",
        ),
        ("on:\n  pull_request:\n    paths: [ 'src/**' ]\n", "`paths`"),
        (
            "on:\n  pull_request:\n    paths-ignore: [ 'docs/**' ]\n",
            "`paths-ignore`",
        ),
        // Path filters stay forbidden on the other triggers of this workflow.
        (
            "on:\n  push:\n    paths: [ 'src/**' ]\n  pull_request:\n",
            "`paths`",
        ),
        (
            "on:\n  pull_request:\n    tags: [ v1 ]\n",
            "unrecognized `pull_request` filter `tags`",
        ),
    ] {
        let v = violations_for(on);
        assert_eq!(v.len(), 1, "{on:?}: {v:?}");
        assert!(v[0].contains(reason), "{on:?}: {v:?}");
    }
}

#[test]
fn trigger_shapes_the_reader_cannot_parse_fail_closed() {
    for on in [
        "on: {pull_request: {branches: [main]}}\n",
        "on: [push,\n  pull_request]\n",
        "on:\n",
        "on:\n  - push\n  - pull_request:\n      branches: [ main ]\n",
        "on:\n  pull_request: {branches: [main]}\n",
        // Equivalent to a bare `pull_request:`, but only block style is read.
        "on:\n  pull_request: {}\n",
        // Valid YAML for `push:`, but not a key the line reader parses.
        "on:\n  pull_request:\n  push :\n",
    ] {
        let v = violations_for(on);
        assert_eq!(v.len(), 1, "{on:?}: {v:?}");
    }
    // A sequence at the key's own indentation is not read as the list value.
    let compact = violations_for("on:\n  pull_request:\n    branches:\n    - main\n");
    assert_eq!(compact.len(), 2, "{compact:?}");
}
