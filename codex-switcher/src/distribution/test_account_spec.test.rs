use super::TestAccountSpec;

#[test]
fn defaults_describe_an_unnamed_depleted_account_without_errors() {
    let account = TestAccountSpec {
        id: "acc",
        email: "acc@example.com",
        plan: "plus",
        ..TestAccountSpec::default()
    }
    .build();
    assert_eq!(account.id, "acc");
    assert_eq!(account.account_id, "acc");
    assert_eq!(account.tokens.account_id.as_deref(), Some("acc"));
    assert_eq!(account.name, None);
    assert_eq!(account.last_primary_percentage, 0.0);
    assert_eq!(account.last_weekly_percentage, None);
    assert_eq!(account.last_credits, Some(0));
    assert_eq!(account.last_reset_after_seconds, None);
    assert_eq!(account.last_error, None);
    assert!(account.enabled);
}

#[test]
fn every_named_field_reaches_the_account() {
    let account = TestAccountSpec {
        id: "acc",
        name: Some("Name"),
        email: "acc@example.com",
        plan: "team",
        sprint_pct: 42.5,
        weekly_pct: Some(10.0),
        credits: 3,
        reset_after: Some(120),
        error: Some("HTTP 429"),
    }
    .build();
    assert_eq!(account.name.as_deref(), Some("Name"));
    assert_eq!(account.email, "acc@example.com");
    assert_eq!(account.plan_type, "team");
    assert_eq!(account.last_primary_percentage, 42.5);
    assert_eq!(account.last_weekly_percentage, Some(10.0));
    assert_eq!(account.last_credits, Some(3));
    assert_eq!(account.last_reset_after_seconds, Some(120));
    assert_eq!(account.last_error.as_deref(), Some("HTTP 429"));
}

#[test]
#[should_panic(expected = "TestAccountSpec requires id, email, and plan")]
fn missing_required_identity_fails_loudly() {
    let _ = TestAccountSpec {
        id: "acc",
        email: "acc@example.com",
        ..TestAccountSpec::default()
    }
    .build();
}
