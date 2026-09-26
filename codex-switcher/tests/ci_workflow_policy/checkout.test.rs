use super::checkout_credential_violations;
use crate::fixtures::with;

#[test]
fn checkout_must_not_persist_credentials() {
    for text in [
        with("          persist-credentials: false\n", "          fetch-depth: 0\n"),
        with("          persist-credentials: false\n", "          persist-credentials: true\n"),
        // The next step's setting must not satisfy this checkout.
        with("        with:\n          persist-credentials: false\n      - run: cargo test\n", "      - uses: x/y@SHA2 # v1.0.0\n        with:\n          persist-credentials: false\n"),
    ] {
        assert_eq!(checkout_credential_violations(&text).len(), 1, "{text}");
    }
    let inline = with("      - name: Checkout\n        uses:", "      - uses:");
    assert_eq!(
        checkout_credential_violations(&inline),
        Vec::<String>::new()
    );
}
