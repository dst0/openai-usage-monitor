use super::rollout_file_name::RolloutFileName;
use super::rollout_scan::RolloutScan;
use super::staging_file::StagingFile;
use super::ROLLOUT_ROOTS;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Walks `sessions/` and `archived_sessions/` for compression candidates.
///
/// Symlinked files and directories are never followed or returned. A plain
/// rollout qualifies when it is older than the minimum age and has no
/// compressed sibling; the compressor rechecks everything under its locks.
pub struct ColdRolloutScanner {
    min_age: Duration,
    now: SystemTime,
}

/// A staging file this old belongs to a run that no longer exists.
const STALE_STAGING_AGE: Duration = Duration::from_secs(24 * 60 * 60);

impl ColdRolloutScanner {
    pub fn new(min_age: Duration, now: SystemTime) -> Self {
        Self { min_age, now }
    }

    pub fn scan(&self, codex_home: &Path) -> RolloutScan {
        let mut scan = RolloutScan::default();
        let mut aged: Vec<(SystemTime, PathBuf)> = Vec::new();
        for root in ROLLOUT_ROOTS {
            let mut stack = vec![codex_home.join(root)];
            while let Some(directory) = stack.pop() {
                let entries = match std::fs::read_dir(&directory) {
                    Ok(entries) => entries,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => {
                        scan.errors += 1;
                        continue;
                    }
                };
                for entry in entries {
                    let Ok(entry) = entry else {
                        scan.errors += 1;
                        continue;
                    };
                    let Ok(file_type) = entry.file_type() else {
                        scan.errors += 1;
                        continue;
                    };
                    let path = entry.path();
                    if file_type.is_dir() {
                        stack.push(path);
                    } else if file_type.is_file() {
                        self.classify(path, &mut scan, &mut aged);
                    }
                }
            }
        }
        aged.sort();
        scan.candidates = aged.into_iter().map(|(_, path)| path).collect();
        scan
    }

    fn classify(&self, path: PathBuf, scan: &mut RolloutScan, aged: &mut Vec<(SystemTime, PathBuf)>) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        if StagingFile::is_stale_name(name) {
            if self.age_of(&path).is_some_and(|age| age >= STALE_STAGING_AGE) {
                scan.stale_staging.push(path);
            }
            return;
        }
        if RolloutFileName::parse(name).is_none()
            || RolloutFileName::compressed_path(&path).symlink_metadata().is_ok()
        {
            return;
        }
        match std::fs::symlink_metadata(&path).and_then(|metadata| metadata.modified()) {
            Ok(modified) if self.now.duration_since(modified).unwrap_or_default() >= self.min_age => {
                aged.push((modified, path));
            }
            Ok(_) => {}
            Err(_) => scan.errors += 1,
        }
    }

    fn age_of(&self, path: &Path) -> Option<Duration> {
        let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
        self.now.duration_since(modified).ok()
    }
}

#[cfg(test)]
#[path = "cold_rollout_scanner.test.rs"]
mod tests;
