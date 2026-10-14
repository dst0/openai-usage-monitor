use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};

/// Reads the thread ID from a rollout's first record, as Codex's compressor
/// does before it takes a thread's writer lock. A name that disagrees with
/// its content is not compressed.
pub struct SessionMetaReader;

/// The first record embeds the base instructions, so it can be large; a line
/// longer than this is treated as unreadable.
const MAX_FIRST_LINE_BYTES: u64 = 16 * 1024 * 1024;

impl SessionMetaReader {
    /// The `payload.id` of a leading `session_meta` record, or `None` when the
    /// first record is missing, oversized, unterminated or of another type.
    pub fn thread_id(file: &File) -> io::Result<Option<String>> {
        let mut line = Vec::new();
        BufReader::new(file.take(MAX_FIRST_LINE_BYTES)).read_until(b'\n', &mut line)?;
        if line.last() != Some(&b'\n') {
            return Ok(None);
        }
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(&line) else {
            return Ok(None);
        };
        if record.get("type").and_then(|value| value.as_str()) != Some("session_meta") {
            return Ok(None);
        }
        Ok(record
            .get("payload")
            .and_then(|payload| payload.get("id"))
            .and_then(|id| id.as_str())
            .map(str::to_ascii_lowercase))
    }
}

#[cfg(test)]
#[path = "session_meta_reader.test.rs"]
mod tests;
