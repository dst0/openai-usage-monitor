pub(super) fn retry(_thread_id: &str) -> Result<(), String> {
    Err("Pinned ChatGPT task navigation is unsupported on this platform".into())
}

pub(crate) fn is_identity_change(_error: &str) -> bool {
    false
}
