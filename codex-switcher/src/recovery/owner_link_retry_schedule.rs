use std::time::Duration;

const ORDINARY_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const PINNED_RETRY_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OwnerLinkRetry {
    Ordinary,
    PinnedNative,
}

pub(super) fn route(
    elapsed: Duration,
    since_ordinary: Duration,
    native_attempted: bool,
) -> Option<OwnerLinkRetry> {
    if elapsed >= PINNED_RETRY_AFTER && !native_attempted {
        Some(OwnerLinkRetry::PinnedNative)
    } else if since_ordinary >= ORDINARY_RETRY_INTERVAL {
        Some(OwnerLinkRetry::Ordinary)
    } else {
        None
    }
}

pub(super) fn deliver(
    selected: OwnerLinkRetry,
    ordinary: impl FnOnce() -> Result<(), String>,
    pinned_native: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match selected {
        OwnerLinkRetry::Ordinary => ordinary(),
        OwnerLinkRetry::PinnedNative => pinned_native(),
    }
}

pub(super) fn retry_thread_link(thread_id: &str, selected: OwnerLinkRetry) -> Result<(), String> {
    deliver(
        selected,
        || crate::switcher::retry_thread_link_in_background(thread_id),
        || crate::switcher::retry_thread_link_natively_in_background(thread_id),
    )
}

#[cfg(test)]
#[path = "owner_link_retry_schedule.test.rs"]
mod tests;
