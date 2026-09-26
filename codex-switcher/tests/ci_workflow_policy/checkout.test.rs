use super::checkout_credential_violations;
use crate::fixtures::{with, SHA};

/// The compliant fixture's checkout inputs.
const WITH_BLOCK: &str = "        with:\n          persist-credentials: false\n";

fn violations_with_inputs(inputs: &str) -> Vec<String> {
    checkout_credential_violations(&with(WITH_BLOCK, inputs))
}

#[test]
fn checkout_must_not_persist_credentials() {
    for text in [
        with("          persist-credentials: false\n", "          fetch-depth: 0\n"),
        with("          persist-credentials: false\n", "          persist-credentials: true\n"),
        // The next step's setting must not satisfy this checkout.
        with("        with:\n          persist-credentials: false\n      - run: cargo test --locked\n", "      - uses: x/y@SHA2 # v1.0.0\n        with:\n          persist-credentials: false\n"),
    ] {
        assert_eq!(checkout_credential_violations(&text).len(), 1, "{text}");
    }
    let inline = with("      - name: Checkout\n        uses:", "      - uses:");
    assert_eq!(
        checkout_credential_violations(&inline),
        Vec::<String>::new()
    );
}

/// Regression (PR #13 review): any descendant line spelling
/// `persist-credentials: false` used to satisfy the rule, although
/// `actions/checkout` reads only its own `with:` input and persists otherwise.
#[test]
fn persist_credentials_must_be_a_direct_with_input() {
    for inputs in [
        // An environment variable is not an action input.
        "        env:\n          persist-credentials: false\n",
        // A step property is not an action input either.
        "        persist-credentials: false\n",
        // Text inside another input's block scalar.
        "        with:\n          fetch-depth: 0\n          sparse-checkout: |\n            persist-credentials: false\n",
        // A nested mapping under some other input.
        "        with:\n          extra:\n            persist-credentials: false\n",
        // A sibling block after `with:`.
        "        with:\n          fetch-depth: 1\n        env:\n          persist-credentials: false\n",
    ] {
        let v = violations_with_inputs(inputs);
        assert_eq!(v.len(), 1, "{inputs:?}: {v:?}");
        assert!(v[0].contains("`persist-credentials: false`"), "{v:?}");
    }
    // An empty `with:` on the step marker line: the sibling properties below
    // it sit past the marker but not past the `with` key, so they are not its
    // inputs.
    let empty_marker_with = with(
        &format!("      - name: Checkout\n        uses: actions/checkout@{SHA} # v4.4.0\n{WITH_BLOCK}"),
        &format!("      - with:\n        uses: actions/checkout@{SHA} # v4.4.0\n        persist-credentials: false\n"),
    );
    assert_eq!(checkout_credential_violations(&empty_marker_with).len(), 1);
}

#[test]
fn ambiguous_or_non_literal_inputs_fail_closed() {
    for (inputs, reason) in [
        ("        with:\n          persist-credentials: false\n          persist-credentials: true\n", "exactly once"),
        ("        with:\n          persist-credentials: false\n        with:\n          fetch-depth: 0\n", "exactly once"),
        ("        with: { persist-credentials: false }\n", "block mapping"),
        ("        with: *checkout-inputs\n", "block mapping"),
        ("        with:\n          persist-credentials: ${{ inputs.persist }}\n", "exactly once"),
        ("        with:\n          persist-credentials:\n", "exactly once"),
        ("        with:\n          persist-credentials: \"true\"\n", "exactly once"),
        ("        with:\n", "exactly once"),
    ] {
        let v = violations_with_inputs(inputs);
        assert_eq!(v.len(), 1, "{inputs:?}: {v:?}");
        assert!(v[0].contains(reason), "{inputs:?}: {v:?}");
    }
}

#[test]
fn equivalent_spellings_and_layouts_comply() {
    for inputs in [
        "        with:\n          persist-credentials: \"false\"\n",
        "        with:\n          persist-credentials: 'false'\n",
        "        with:\n          persist-credentials: false # keep the token out of .git\n",
        "        with:\n          \"persist-credentials\": false\n",
        "        with: # inputs\n          sparse-checkout: |\n            src\n            persist-credentials: true\n          persist-credentials: false\n",
    ] {
        assert_eq!(
            violations_with_inputs(inputs),
            Vec::<String>::new(),
            "{inputs:?}"
        );
    }
    let uses = format!("        uses: actions/checkout@{SHA} # v4.4.0\n");
    // `with:` may precede `uses:`, on the step marker line or below it.
    for step in [
        format!("      - name: Checkout\n{WITH_BLOCK}{uses}"),
        format!("      - with:\n          persist-credentials: false\n{uses}"),
        format!("      -   name: Checkout\n          uses: actions/checkout@{SHA} # v4.4.0\n          with:\n            persist-credentials: false\n"),
    ] {
        let text = with(&format!("      - name: Checkout\n{uses}{WITH_BLOCK}"), &step);
        assert_eq!(checkout_credential_violations(&text), Vec::<String>::new(), "{step}");
    }
}

#[test]
fn checkout_outside_a_step_or_in_another_spelling_is_still_checked() {
    // GitHub resolves owner and repository names case-insensitively, and a
    // path after the repository still names the same repository.
    let reference = format!("actions/checkout@{SHA}");
    for spelling in [
        format!("Actions/Checkout@{SHA}"),
        format!("actions/checkout/@{SHA}"),
        format!("actions/checkout/.@{SHA}"),
        "actions/checkout".to_string(),
    ] {
        let text = with(&reference, &spelling);
        assert_eq!(
            checkout_credential_violations(&text),
            Vec::<String>::new(),
            "{spelling}"
        );
        let persisted = text.replace("persist-credentials: false", "fetch-depth: 0");
        assert_eq!(
            checkout_credential_violations(&persisted).len(),
            1,
            "{spelling}"
        );
    }
    // Other repositories are not checkouts.
    for other in [
        "actions/checkout-extra",
        "my-actions/checkout",
        "actions/cache",
    ] {
        let text = with(&reference, &format!("{other}@{SHA}"))
            .replace("persist-credentials: false", "fetch-depth: 0");
        assert_eq!(
            checkout_credential_violations(&text),
            Vec::<String>::new(),
            "{other}"
        );
    }
    // A checkout that is not a step list item cannot be tied to its inputs.
    let job_level = with(
        "    timeout-minutes: 5\n",
        &format!("    timeout-minutes: 5\n    uses: actions/checkout@{SHA} # v4.4.0\n    with:\n      persist-credentials: false\n"),
    );
    assert_eq!(checkout_credential_violations(&job_level).len(), 1);
}

/// libyaml-based parsers (PyYAML, Ruby Psych) read these shapes differently
/// from the line reader: an unterminated quoted scalar turns the following
/// `with:` block into text, and a `<<` merge adds a checkout step that has no
/// `uses:` line of its own. The whole-workflow scan must reject them.
#[test]
fn continued_or_merged_nodes_cannot_fake_checkout_inputs() {
    let merged = format!(
        "      - &checkout\n        name: Checkout\n        uses: actions/checkout@{SHA} # v4.4.0\n{WITH_BLOCK}      - <<: *checkout\n        with:\n          fetch-depth: 0\n"
    );
    for (text, marker) in [
        (with(WITH_BLOCK, &format!("        env:\n          NOTE: \"inputs follow\n{WITH_BLOCK}          \"\n")), "NOTE: \""),
        (with(WITH_BLOCK, &format!("        env:\n          NOTE: 'inputs follow\n{WITH_BLOCK}          '\n")), "NOTE: '"),
        (with(&format!("      - name: Checkout\n        uses: actions/checkout@{SHA} # v4.4.0\n{WITH_BLOCK}"), &merged), "<<: *checkout"),
    ] {
        let line = 1 + text.lines().position(|l| l.contains(marker)).expect("marker");
        let v = crate::rules::workflow_violations(&text);
        assert!(
            v.iter().any(|m| m.starts_with(&format!("line {line}: "))),
            "{text}\n{v:?}"
        );
    }
}

/// `workflow_violations` runs the checkout rule, so the live workflow test
/// cannot pass with a persisting checkout.
#[test]
fn workflow_violations_include_the_checkout_rule() {
    let persisted = with(
        "          persist-credentials: false\n",
        "          fetch-depth: 0\n",
    );
    let v = crate::rules::workflow_violations(&persisted);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].starts_with("checkout at line 17: "), "{v:?}");
}
