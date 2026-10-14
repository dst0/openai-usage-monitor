use std::path::Path;

/// Points in the compression protocol where tests interleave a concurrent
/// writer. Production uses `NoCompressionCheckpoints`.
pub trait CompressionCheckpoints {
    /// After the frame is encoded and verified, before any lock is taken.
    fn after_encode(&self, _source: &Path) {}

    /// After the compressed file is published, before the source is removed.
    fn before_remove_source(&self, _source: &Path) {}
}
