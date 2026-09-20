use std::path::PathBuf;

pub struct ArchiveResult {
    pub archive_path: PathBuf,
    pub original_bytes: u64,
    pub compressed_bytes: u64,
}
