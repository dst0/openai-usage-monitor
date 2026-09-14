use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use chrono::Utc;
use fs2::FileExt;

pub const DEFAULT_MAX_LOG_SIZE: u64 = 512 * 1024; // 512 KB
pub const DEFAULT_MAX_ARCHIVES: usize = 20;
pub const BROTLI_QUALITY: u32 = 6;
pub const BROTLI_LGWIN: u32 = 22;

pub fn log_dir() -> PathBuf {
    crate::storage::codex_home().join("log")
}

pub fn switcher_log_path() -> PathBuf {
    log_dir().join("switcher.log")
}

pub fn archive_dir() -> PathBuf {
    log_dir().join("archive")
}

/// Compresses arbitrary byte data with Brotli Quality 6 (same configuration used
/// across benchmark evidence and recovery testing).
pub fn compress_brotli_q6(input: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut writer = brotli::CompressorWriter::new(&mut output, 4096, BROTLI_QUALITY, BROTLI_LGWIN);
    writer.write_all(input)?;
    writer.flush()?;
    drop(writer);
    Ok(output)
}

/// Decompresses Brotli data.
pub fn decompress_brotli(input: &[u8]) -> io::Result<Vec<u8>> {
    let mut reader = brotli::Decompressor::new(input, 4096);
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

/// Appends a structured log entry to `~/.codex/log/switcher.log` with POSIX 0600 permissions
/// and exclusive file flock.
pub fn log(level: &str, category: &str, message: &str) {
    let timestamp = Utc::now().to_rfc3339();
    let line = format!("{timestamp} [{level}] [{category}] {message}\n");

    let path = switcher_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        if let Ok(metadata) = fs::metadata(parent) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(parent, perms);
        }
    }

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
    {
        if file.lock_exclusive().is_ok() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.unlock();
        }
    }
}

#[macro_export]
macro_rules! log_info {
    ($cat:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
        $crate::logger::log("INFO", $cat, &msg);
    }};
}

#[macro_export]
macro_rules! log_warn {
    ($cat:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        eprintln!("{}", msg);
        $crate::logger::log("WARN", $cat, &msg);
    }};
}

#[macro_export]
macro_rules! log_error {
    ($cat:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        eprintln!("{}", msg);
        $crate::logger::log("ERROR", $cat, &msg);
    }};
}

pub struct ArchiveResult {
    pub archive_path: PathBuf,
    pub original_bytes: u64,
    pub compressed_bytes: u64,
}

/// Checks a log file and, if its size exceeds `max_size_bytes`, rotates it by
/// compressing the content with Brotli Q6 into `~/.codex/log/archive/`, truncating the
/// original file (preserving launchd/process append handles), and pruning older archives.
pub fn rotate_file_if_needed(
    path: &Path,
    archive_prefix: &str,
    max_size_bytes: u64,
    max_archives: usize,
) -> io::Result<Option<ArchiveResult>> {
    if !path.is_file() {
        return Ok(None);
    }
    let size = fs::metadata(path)?.len();
    if size < max_size_bytes {
        return Ok(None);
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;

    file.lock_exclusive()?;

    // Re-check size under lock in case another process already rotated
    let size = file.metadata()?.len();
    if size < max_size_bytes || size == 0 {
        let _ = file.unlock();
        return Ok(None);
    }

    let mut content = Vec::with_capacity(size as usize);
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut content)?;

    if content.is_empty() {
        let _ = file.unlock();
        return Ok(None);
    }

    let compressed = compress_brotli_q6(&content)?;
    let compressed_len = compressed.len() as u64;

    let archives_dir = archive_dir();
    fs::create_dir_all(&archives_dir)?;
    if let Ok(metadata) = fs::metadata(&archives_dir) {
        let mut perms = metadata.permissions();
        perms.set_mode(0o700);
        let _ = fs::set_permissions(&archives_dir, perms);
    }

    let now_str = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let archive_file_name = format!("{archive_prefix}-{now_str}.log.br");
    let archive_path = archives_dir.join(&archive_file_name);

    let mut archive_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&archive_path)?;
    archive_file.write_all(&compressed)?;
    archive_file.sync_all()?;
    drop(archive_file);

    // Truncate original file to 0 bytes while preserving active handles (e.g. launchd stdio)
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    let _ = file.unlock();

    // Prune older archives
    prune_archives(&archives_dir, archive_prefix, max_archives);

    Ok(Some(ArchiveResult {
        archive_path,
        original_bytes: content.len() as u64,
        compressed_bytes: compressed_len,
    }))
}

/// Rotates switcher.log as well as daemon stdout and stderr logs if they exceed max size.
pub fn rotate_all_logs(max_size_bytes: u64, max_archives: usize) -> Vec<ArchiveResult> {
    let mut results = Vec::new();

    let switcher_log = switcher_log_path();
    if let Ok(Some(res)) = rotate_file_if_needed(&switcher_log, "switcher", max_size_bytes, max_archives) {
        results.push(res);
    }

    let daemon_log = crate::storage::codex_home().join("account-switcher-daemon.log");
    if let Ok(Some(res)) = rotate_file_if_needed(&daemon_log, "account-switcher-daemon", max_size_bytes, max_archives) {
        results.push(res);
    }

    let daemon_err = crate::storage::codex_home().join("account-switcher-daemon.err");
    if let Ok(Some(res)) = rotate_file_if_needed(&daemon_err, "account-switcher-daemon-err", max_size_bytes, max_archives) {
        results.push(res);
    }

    results
}

fn prune_archives(dir: &Path, prefix: &str, keep_count: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut matching_files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                name.starts_with(prefix) && name.ends_with(".log.br")
            } else {
                false
            }
        })
        .collect();

    if matching_files.len() <= keep_count {
        return;
    }

    // Sort ascending by filename (which includes YYYYMMDD-HHMMSS timestamp)
    matching_files.sort();

    let to_remove = matching_files.len().saturating_sub(keep_count);
    for file in matching_files.into_iter().take(to_remove) {
        let _ = fs::remove_file(file);
    }
}

pub struct ArchiveInfo {
    pub filename: String,
    pub path: PathBuf,
    pub compressed_size: u64,
    pub uncompressed_size: Option<u64>,
    pub savings_percent: Option<f64>,
}

/// Lists all compressed log archives with sizes and compression stats.
pub fn list_archives() -> io::Result<Vec<ArchiveInfo>> {
    let dir = archive_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut archives = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("br") {
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let compressed_size = entry.metadata()?.len();
            let (uncompressed_size, savings_percent) = match fs::read(&path) {
                Ok(compressed_bytes) => match decompress_brotli(&compressed_bytes) {
                    Ok(decompressed) => {
                        let uncompressed_len = decompressed.len() as u64;
                        let savings = if uncompressed_len > 0 {
                            (1.0 - (compressed_size as f64 / uncompressed_len as f64)) * 100.0
                        } else {
                            0.0
                        };
                        (Some(uncompressed_len), Some(savings))
                    }
                    Err(_) => (None, None),
                },
                Err(_) => (None, None),
            };

            archives.push(ArchiveInfo {
                filename,
                path,
                compressed_size,
                uncompressed_size,
                savings_percent,
            });
        }
    }

    archives.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(archives)
}

/// Reads the last N lines from the switcher log file.
pub fn read_recent_logs(lines: usize) -> io::Result<Vec<String>> {
    let path = switcher_log_path();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)?;
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    Ok(all_lines[start..].iter().map(|s| s.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brotli_q6_compression_and_decompression_roundtrip() {
        let original_data = b"2026-09-14T21:38:00Z [INFO] [USER_SWITCH] Switching to dst-2@destinationworks.com.au\n\
                              2026-09-14T21:38:05Z [INFO] [RECOVERY] RECOVERY_STARTED thread=01a0979a\n\
                              2026-09-14T21:38:15Z [INFO] [RECOVERY] RECOVERY_VERIFIED thread=01a0979a stable_secs=10\n".repeat(20);
        let compressed = compress_brotli_q6(&original_data).expect("Compression should succeed");
        assert!(compressed.len() < original_data.len(), "Brotli should compress repeated logs significantly");
        let decompressed = decompress_brotli(&compressed).expect("Decompression should succeed");
        assert_eq!(decompressed, original_data);
    }

    #[test]
    fn test_rotate_file_if_needed_compresses_and_truncates() {
        let temp_dir = std::env::temp_dir().join(format!("test_log_rotate_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let log_file = temp_dir.join("test_switcher.log");

        let test_data = "Log line entry for test rotation\n".repeat(100);
        fs::write(&log_file, &test_data).unwrap();

        let initial_size = fs::metadata(&log_file).unwrap().len();
        assert!(initial_size > 1000);

        // Threshold set to 500 bytes -> should trigger rotation
        let res = rotate_file_if_needed(&log_file, "test-archive", 500, 5)
            .expect("Rotation should succeed")
            .expect("Should return ArchiveResult");

        assert_eq!(res.original_bytes, initial_size);
        assert!(res.compressed_bytes < initial_size);
        assert!(res.archive_path.exists());
        assert!(res.archive_path.to_str().unwrap().ends_with(".log.br"));

        // Original file should be truncated to 0 bytes
        let truncated_size = fs::metadata(&log_file).unwrap().len();
        assert_eq!(truncated_size, 0);

        // Clean up
        let _ = fs::remove_file(res.archive_path);
        let _ = fs::remove_file(log_file);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_prune_archives_bounds_retention() {
        let temp_dir = std::env::temp_dir().join(format!("test_prune_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        for i in 1..=10 {
            let file = temp_dir.join(format!("test-prefix-20260914-00000{i:02}.log.br"));
            fs::write(&file, b"test").unwrap();
        }

        prune_archives(&temp_dir, "test-prefix", 5);

        let remaining: Vec<_> = fs::read_dir(&temp_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert_eq!(remaining.len(), 5);
        let _ = fs::remove_dir_all(temp_dir);
    }
}
