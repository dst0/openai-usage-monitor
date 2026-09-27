/// A thread-index `updated_at` lookup for prune-pass tests, dated before the
/// pass that uses it.
///
/// Production reads the SQLite thread index before a prune pass samples its
/// clock. A fixture that sampled the clock inside the lookup ran after the
/// pass did, so across a second boundary it dated the row one second in the
/// future. The pass treats no future-dated row as recent and dropped the
/// ownerless retry the test expected to keep. A minute's margin also absorbs
/// a small wall-clock step.
pub(super) fn indexed_before_the_pass() -> impl Fn(&str) -> Result<Option<i64>, String> + Copy {
    let updated_at = chrono::Utc::now().timestamp() - 60;
    move |_| Ok(Some(updated_at))
}
