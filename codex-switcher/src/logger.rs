use crate::distribution::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use crate::distribution::{LogRedactionService, MonitorLogIoService};
use crate::logger_archive_info::ArchiveInfo;
use crate::logger_archive_result::ArchiveResult;
use chrono::Utc;
use fs2::FileExt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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
    let clean_level = LogRedactionService::sanitize_text(level);
    let clean_category = LogRedactionService::sanitize_text(category);
    let clean_message = LogRedactionService::sanitize_text(message);
    let line = format!("{timestamp} [{clean_level}] [{clean_category}] {clean_message}\n");

    let _ = MonitorLogIoService::append(&switcher_log_path(), line.as_bytes());
}

#[macro_export]
macro_rules! log_info {
    ($cat:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        $crate::distribution::LogRedactionService::print_runtime(&msg);
        $crate::logger::log("INFO", $cat, &msg);
    }};
}

#[macro_export]
macro_rules! log_warn {
    ($cat:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        $crate::distribution::LogRedactionService::eprint_runtime(&msg);
        $crate::logger::log("WARN", $cat, &msg);
    }};
}

#[macro_export]
macro_rules! log_error {
    ($cat:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        $crate::distribution::LogRedactionService::eprint_runtime(&msg);
        $crate::logger::log("ERROR", $cat, &msg);
    }};
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
    let _lifecycle = MonitorLogLifecycleLock::shared_for_log(path)?;
    let Some((mut file, archive_dir)) = MonitorLogIoService::open_for_rotation(path)? else {
        return Ok(None);
    };

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

    let now_str = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let archive_file_name = format!("{archive_prefix}-{now_str}.log.br");
    let archive_path = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log has no parent"))?
        .join("archive")
        .join(&archive_file_name);

    let mut archive_file =
        MonitorLogIoService::create_archive_file(&archive_dir, &archive_file_name)?;
    archive_file.write_all(&compressed)?;
    archive_file.sync_all()?;
    drop(archive_file);

    // Truncate original file to 0 bytes while preserving active handles (e.g. launchd stdio)
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    let _ = file.unlock();

    // Prune older archives
    prune_archives(&archive_dir, archive_prefix, max_archives);

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
    if let Ok(Some(res)) =
        rotate_file_if_needed(&switcher_log, "switcher", max_size_bytes, max_archives)
    {
        results.push(res);
    }

    let daemon_log = crate::storage::codex_home().join("account-switcher-daemon.log");
    if let Ok(Some(res)) = rotate_file_if_needed(
        &daemon_log,
        "account-switcher-daemon",
        max_size_bytes,
        max_archives,
    ) {
        results.push(res);
    }

    let daemon_err = crate::storage::codex_home().join("account-switcher-daemon.err");
    if let Ok(Some(res)) = rotate_file_if_needed(
        &daemon_err,
        "account-switcher-daemon-err",
        max_size_bytes,
        max_archives,
    ) {
        results.push(res);
    }

    results
}

fn prune_archives(dir: &File, prefix: &str, keep_count: usize) {
    let Ok(entries) = MonitorLogIoService::archive_names(dir) else {
        return;
    };
    let mut matching_files: Vec<String> = entries
        .into_iter()
        .filter(|name| archive_name_matches(name, prefix))
        .collect();

    if matching_files.len() <= keep_count {
        return;
    }

    // Sort ascending by filename (which includes YYYYMMDD-HHMMSS timestamp)
    matching_files.sort();

    let to_remove = matching_files.len().saturating_sub(keep_count);
    for name in matching_files.into_iter().take(to_remove) {
        let _ = MonitorLogIoService::remove_child(dir, &name, false);
    }
}

fn archive_name_matches(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(&format!("{prefix}-")) else {
        return false;
    };
    let Some(timestamp) = rest.strip_suffix(".log.br") else {
        return false;
    };
    timestamp.len() == 15
        && timestamp.as_bytes()[8] == b'-'
        && timestamp
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
}

/// Lists all compressed log archives with sizes and compression stats.
pub fn list_archives() -> io::Result<Vec<ArchiveInfo>> {
    let dir = archive_dir();
    let Some(archive_directory) = MonitorLogIoService::open_directory_path(&dir, false)
        .map(Some)
        .or_else(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            }
        })?
    else {
        return Ok(Vec::new());
    };
    let mut archives = Vec::new();
    for filename in MonitorLogIoService::archive_names(&archive_directory)? {
        if !filename.ends_with(".br") {
            continue;
        }
        let file = MonitorLogIoService::open_archive_file(&archive_directory, &filename)?;
        let compressed_size = file.metadata()?.len();
        let compressed_bytes = MonitorLogIoService::read_to_end(file)?;
        let (uncompressed_size, savings_percent) = match decompress_brotli(&compressed_bytes) {
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
        };

        archives.push(ArchiveInfo {
            path: dir.join(&filename),
            filename,
            compressed_size,
            uncompressed_size,
            savings_percent,
        });
    }

    archives.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(archives)
}

/// Reads the last N lines from the switcher log file.
pub fn read_recent_logs(lines: usize) -> io::Result<Vec<String>> {
    let path = switcher_log_path();
    let Some(mut file) = MonitorLogIoService::open_for_read(&path)? else {
        return Ok(Vec::new());
    };

    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);
    Ok(all_lines[start..].iter().map(|s| s.to_string()).collect())
}

#[cfg(test)]
#[path = "logger.test.rs"]
mod tests;
