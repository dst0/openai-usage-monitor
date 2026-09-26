use super::{deliver, route, OwnerLinkRetry};
use std::time::Duration;

#[test]
fn native_retry_is_wall_clock_bounded_and_once_only() {
    assert_eq!(
        route(Duration::from_secs(9), Duration::from_secs(1), false),
        None
    );
    assert_eq!(
        route(Duration::from_secs(10), Duration::from_secs(0), false),
        Some(OwnerLinkRetry::PinnedNative)
    );
    assert_eq!(
        route(Duration::from_secs(11), Duration::from_secs(0), true),
        None
    );
    assert_eq!(
        route(Duration::from_secs(12), Duration::from_secs(2), true),
        Some(OwnerLinkRetry::Ordinary)
    );
}

#[test]
fn ordinary_retry_remains_available_before_native_fallback() {
    assert_eq!(
        route(Duration::from_secs(1), Duration::from_secs(1), false),
        None
    );
    assert_eq!(
        route(Duration::from_secs(2), Duration::from_secs(2), false),
        Some(OwnerLinkRetry::Ordinary)
    );
}

#[test]
fn failed_native_delivery_keeps_ordinary_route_usable() {
    assert!(deliver(
        OwnerLinkRetry::PinnedNative,
        || panic!("ordinary link must not run during native fallback"),
        || Err("navigation unavailable".into()),
    )
    .is_err());
    assert!(deliver(
        OwnerLinkRetry::Ordinary,
        || Ok(()),
        || panic!("native fallback must not repeat for ordinary retry"),
    )
    .is_ok());
}
