use super::compression_checkpoints::CompressionCheckpoints;

/// The production checkpoints: nothing runs between protocol steps.
pub struct NoCompressionCheckpoints;

impl CompressionCheckpoints for NoCompressionCheckpoints {}
