use super::log_redaction_service::LogRedactionService;
use crate::logger::{BROTLI_LGWIN, BROTLI_QUALITY};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

const MAX_LEGACY_LOG_LINE_BYTES: usize = 1024 * 1024;
const MAX_BOUNDED_LINE_BYTES: usize = MAX_LEGACY_LOG_LINE_BYTES + 1;

pub(super) fn rewrite_plain(source: File, target: &mut File) -> io::Result<bool> {
    let mut reader = BufReader::new(source);
    sanitize_stream(&mut reader, target)
}

pub(super) fn rewrite_brotli(source: File, target: &mut File) -> io::Result<bool> {
    let decompressor = brotli::Decompressor::new(source, 4096);
    let mut reader = BufReader::new(decompressor);
    let mut compressor = brotli::CompressorWriter::new(target, 4096, BROTLI_QUALITY, BROTLI_LGWIN);
    let changed = sanitize_stream(&mut reader, &mut compressor)?;
    compressor.flush()?;
    Ok(changed)
}

fn sanitize_stream(reader: &mut dyn BufRead, writer: &mut dyn Write) -> io::Result<bool> {
    let mut line = Vec::with_capacity(MAX_BOUNDED_LINE_BYTES);
    let mut changed = false;
    loop {
        line.clear();
        if read_bounded_line(reader, &mut line)? == 0 {
            return Ok(changed);
        }
        let had_newline = line.last() == Some(&b'\n');
        if had_newline {
            line.pop();
        }
        let had_carriage_return = line.last() == Some(&b'\r');
        if had_carriage_return {
            line.pop();
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "log is not UTF-8"))?;
        let sanitized = LogRedactionService::sanitize_text(text);
        changed |= had_carriage_return || sanitized.as_bytes() != line;
        writer.write_all(sanitized.as_bytes())?;
        if had_newline {
            writer.write_all(b"\n")?;
        }
    }
}

fn read_bounded_line(reader: &mut dyn BufRead, line: &mut Vec<u8>) -> io::Result<usize> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(line.len());
        }
        let newline_end = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1);
        let remaining = MAX_BOUNDED_LINE_BYTES.saturating_sub(line.len());
        let take = newline_end.unwrap_or(buffer.len()).min(remaining);
        if take == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy log line exceeds safety bound",
            ));
        }
        let found_newline = newline_end.is_some_and(|end| end <= take);
        line.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if found_newline {
            if line.len() > MAX_LEGACY_LOG_LINE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "legacy log line exceeds safety bound",
                ));
            }
            return Ok(line.len());
        }
        if line.len() == MAX_BOUNDED_LINE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy log line exceeds safety bound",
            ));
        }
    }
}
