use crate::{distribution, logger};

pub(super) struct LogCommandService;

impl LogCommandService {
    pub(super) fn logs(lines: usize, archives: bool, rotate: bool) -> Result<(), String> {
        if rotate {
            println!("🔄 Rotating switcher logs with Brotli Q6 compression...");
            let results = logger::rotate_all_logs(0, logger::DEFAULT_MAX_ARCHIVES);
            if results.is_empty() {
                println!("ℹ️ No non-empty log files found to rotate.");
            } else {
                for result in results {
                    let savings = if result.original_bytes > 0 {
                        (1.0 - (result.compressed_bytes as f64 / result.original_bytes as f64))
                            * 100.0
                    } else {
                        0.0
                    };
                    println!(
                        "📦 Archived: {} ({} -> {} bytes, {:.1}% saved)",
                        result
                            .archive_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        result.original_bytes,
                        result.compressed_bytes,
                        savings
                    );
                }
            }
            return Ok(());
        }
        if archives {
            return Self::list_archives();
        }
        let recent = logger::read_recent_logs(lines).map_err(|error| error.to_string())?;
        if recent.is_empty() {
            println!("ℹ️ No logs found in ~/.codex/log/switcher.log");
        } else {
            for line in recent {
                println!("{}", line);
            }
        }
        Ok(())
    }

    pub(super) fn monitor_logs(
        install: bool,
        remove: bool,
        dry_run: bool,
        purge_data: bool,
        cancel: bool,
    ) -> Result<(), String> {
        let modes = [install, remove, dry_run, cancel]
            .iter()
            .filter(|enabled| **enabled)
            .count();
        if modes != 1 {
            Err("monitor log command requires exactly one action".into())
        } else if install {
            distribution::MonitorLogCleanupService::install()
        } else if remove {
            distribution::MonitorLogCleanupService::remove(purge_data)
        } else if cancel {
            distribution::MonitorLogCleanupService::cancel_recovery()
        } else {
            distribution::MonitorLogCleanupService::print_plan(purge_data)
        }
    }

    fn list_archives() -> Result<(), String> {
        let list = logger::list_archives().map_err(|error| error.to_string())?;
        if list.is_empty() {
            println!("ℹ️ No compressed log archives found in ~/.codex/log/archive/.");
            return Ok(());
        }
        println!("\n📦 Brotli-Compressed Log Archives (Q6):");
        println!(
            "{:<44} {:>12} {:>14} {:>10}",
            "ARCHIVE", "COMPRESSED", "UNCOMPRESSED", "SAVINGS"
        );
        println!("{}", "-".repeat(84));
        for archive in list {
            let uncompressed = archive
                .uncompressed_size
                .map(|size| format!("{size} B"))
                .unwrap_or_else(|| "--".into());
            let savings = archive
                .savings_percent
                .map(|percent| format!("{percent:.1}%"))
                .unwrap_or_else(|| "--".into());
            println!(
                "{:<44} {:>10} B {:>14} {:>10}",
                archive.filename, archive.compressed_size, uncompressed, savings
            );
        }
        println!();
        Ok(())
    }
}
