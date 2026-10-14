use super::rollout_file_name::RolloutFileName;
use super::zstd_rollout_codec::ZstdRolloutCodec;
use super::ROLLOUT_ROOTS;
use std::fs::File;
use std::path::Path;

/// How Codex's rollouts are stored right now: plain and compressed counts
/// and sizes, with the original size of compressed ones from their headers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RolloutStorageReport {
    pub plain_files: u64,
    pub plain_bytes: u64,
    pub compressed_files: u64,
    pub compressed_bytes: u64,
    /// Decoded size of compressed rollouts whose header records it.
    pub compressed_original_bytes: u64,
}

impl RolloutStorageReport {
    pub fn collect(codex_home: &Path) -> Self {
        let mut report = Self::default();
        for root in ROLLOUT_ROOTS {
            let mut stack = vec![codex_home.join(root)];
            while let Some(directory) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let Ok(file_type) = entry.file_type() else {
                        continue;
                    };
                    if file_type.is_dir() {
                        stack.push(entry.path());
                    } else if file_type.is_file() {
                        report.add(&entry.path());
                    }
                }
            }
        }
        report
    }

    fn add(&mut self, path: &Path) {
        let Ok(len) = std::fs::symlink_metadata(path).map(|metadata| metadata.len()) else {
            return;
        };
        if RolloutFileName::from_path(path).is_some() {
            self.plain_files += 1;
            self.plain_bytes += len;
        } else if RolloutFileName::plain_path(path).is_some() {
            self.compressed_files += 1;
            self.compressed_bytes += len;
            self.compressed_original_bytes += File::open(path)
                .and_then(|mut file| ZstdRolloutCodec::frame_content_size(&mut file))
                .ok()
                .flatten()
                .unwrap_or(0);
        }
    }

    /// Lines for `cxi sessions status`.
    pub fn render(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Plain rollouts:      {:>6}  {}", self.plain_files, human(self.plain_bytes)),
            format!(
                "Compressed rollouts: {:>6}  {} (originally {})",
                self.compressed_files,
                human(self.compressed_bytes),
                human(self.compressed_original_bytes)
            ),
        ];
        if self.compressed_original_bytes > self.compressed_bytes {
            lines.push(format!(
                "Saved:                       {}",
                human(self.compressed_original_bytes - self.compressed_bytes)
            ));
        }
        lines
    }
}

/// Bytes in the largest binary unit that keeps the value at least 1.
pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
#[path = "rollout_storage_report.test.rs"]
mod tests;
