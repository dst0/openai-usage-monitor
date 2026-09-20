use std::path::PathBuf;

pub struct ArchiveInfo {
    pub filename: String,
    pub path: PathBuf,
    pub compressed_size: u64,
    pub uncompressed_size: Option<u64>,
    pub savings_percent: Option<f64>,
}
