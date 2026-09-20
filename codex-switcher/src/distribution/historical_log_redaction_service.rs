use super::log_redaction_service::LogRedactionService;
use super::monitor_log_io_service::MonitorLogIoService;
use crate::logger::{BROTLI_LGWIN, BROTLI_QUALITY};
use fs2::FileExt;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_LEGACY_LOG_LINE_BYTES: usize = 1024 * 1024;
const ARCHIVE_PREFIXES: [&str; 3] = [
    "switcher",
    "account-switcher-daemon",
    "account-switcher-daemon-err",
];
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct HistoricalLogRedactionService;

impl HistoricalLogRedactionService {
    pub fn sanitize_home(home: &Path) -> Result<(), String> {
        for path in [
            home.join("log/switcher.log"),
            home.join("account-switcher-daemon.log"),
            home.join("account-switcher-daemon.err"),
        ] {
            Self::rewrite_path(&path, false).map_err(|_| Self::failure("active log"))?;
        }

        let archive_path = home.join("log/archive");
        let archive = MonitorLogIoService::open_directory_path(&archive_path, false)
            .map_err(|_| Self::failure("archive directory"))?;
        for name in MonitorLogIoService::archive_names(&archive)
            .map_err(|_| Self::failure("archive directory"))?
        {
            if Self::is_owned_archive(&name) {
                Self::rewrite_child(&archive, &name, true)
                    .map_err(|_| Self::failure("Brotli archive"))?;
            }
        }
        Ok(())
    }

    fn rewrite_path(path: &Path, compressed: bool) -> io::Result<()> {
        let parent_path = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
        let parent = MonitorLogIoService::open_directory_path(parent_path, false)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid filename"))?;
        Self::rewrite_child(&parent, name, compressed)
    }

    fn rewrite_child(parent: &File, name: &str, compressed: bool) -> io::Result<()> {
        let source =
            MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC)?;
        source.lock_exclusive()?;
        let source_metadata = source.metadata()?;
        let reader_source = source.try_clone()?;
        let (temporary_name, mut temporary) = Self::create_temporary(parent, name)?;
        let rewrite_result = if compressed {
            Self::rewrite_brotli(reader_source, &mut temporary)
        } else {
            Self::rewrite_plain(reader_source, &mut temporary)
        };

        if let Err(error) = rewrite_result {
            let _ = MonitorLogIoService::remove_child(parent, &temporary_name, false);
            return Err(error);
        }
        temporary.sync_all()?;
        drop(temporary);

        let final_source_metadata = source.metadata()?;
        if final_source_metadata.len() != source_metadata.len()
            || final_source_metadata.mtime() != source_metadata.mtime()
            || final_source_metadata.mtime_nsec() != source_metadata.mtime_nsec()
        {
            let _ = MonitorLogIoService::remove_child(parent, &temporary_name, false);
            let _ = source.unlock();
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "log changed during redaction",
            ));
        }

        let current =
            MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC)?;
        let current_metadata = current.metadata()?;
        if current_metadata.dev() != source_metadata.dev()
            || current_metadata.ino() != source_metadata.ino()
        {
            let _ = MonitorLogIoService::remove_child(parent, &temporary_name, false);
            let _ = source.unlock();
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "log changed during redaction",
            ));
        }
        drop(current);

        if let Err(error) = Self::rename_child(parent, &temporary_name, name) {
            let _ = MonitorLogIoService::remove_child(parent, &temporary_name, false);
            let _ = source.unlock();
            return Err(error);
        }
        let _ = source.unlock();
        parent.sync_all()
    }

    fn rewrite_plain(source: File, target: &mut File) -> io::Result<()> {
        let mut reader = BufReader::new(source);
        Self::sanitize_stream(&mut reader, target)
    }

    fn rewrite_brotli(source: File, target: &mut File) -> io::Result<()> {
        let decompressor = brotli::Decompressor::new(source, 4096);
        let mut reader = BufReader::new(decompressor);
        let mut compressor =
            brotli::CompressorWriter::new(target, 4096, BROTLI_QUALITY, BROTLI_LGWIN);
        Self::sanitize_stream(&mut reader, &mut compressor)?;
        compressor.flush()
    }

    fn sanitize_stream(reader: &mut dyn BufRead, writer: &mut dyn Write) -> io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 {
                return Ok(());
            }
            if line.len() > MAX_LEGACY_LOG_LINE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "legacy log line exceeds safety bound",
                ));
            }
            let had_newline = line.last() == Some(&b'\n');
            if had_newline {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let text = std::str::from_utf8(&line)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "log is not UTF-8"))?;
            writer.write_all(LogRedactionService::sanitize_text(text).as_bytes())?;
            if had_newline {
                writer.write_all(b"\n")?;
            }
        }
    }

    fn create_temporary(parent: &File, name: &str) -> io::Result<(String, File)> {
        for _ in 0..100 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary_name = format!(".redact-{}-{sequence}.tmp", std::process::id());
            match MonitorLogIoService::open_child_file(
                parent,
                &temporary_name,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
            ) {
                Ok(file) => return Ok((temporary_name, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("could not allocate temporary file for {name}"),
        ))
    }

    fn rename_child(parent: &File, from: &str, to: &str) -> io::Result<()> {
        let from = CString::new(from)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid source name"))?;
        let to = CString::new(to)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid target name"))?;
        let result = unsafe {
            libc::renameat(
                parent.as_raw_fd(),
                from.as_ptr(),
                parent.as_raw_fd(),
                to.as_ptr(),
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn is_owned_archive(name: &str) -> bool {
        ARCHIVE_PREFIXES
            .iter()
            .any(|prefix| Self::timestamped_name(name, prefix))
    }

    fn timestamped_name(name: &str, prefix: &str) -> bool {
        let Some(timestamp) = name
            .strip_prefix(&format!("{prefix}-"))
            .and_then(|rest| rest.strip_suffix(".log.br"))
        else {
            return false;
        };
        timestamp.len() == 15
            && timestamp.as_bytes()[8] == b'-'
            && timestamp
                .bytes()
                .enumerate()
                .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
    }

    fn failure(kind: &str) -> String {
        format!("could not safely redact pre-existing Monitor {kind}")
    }
}
