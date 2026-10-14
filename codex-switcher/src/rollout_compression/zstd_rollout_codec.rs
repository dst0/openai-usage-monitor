use super::content_digest::ContentDigest;
use std::io::{self, Read, Write};
use zstd::zstd_safe::CParameter;

/// Encodes and decodes rollouts as one standard zstd frame that Codex reads.
///
/// Rollouts repeat large blocks far apart (full command output in both the
/// model and UI records, compaction snapshots), so long-distance matching
/// roughly doubles the ratio over plain level 3 at the same speed. The 16 MiB
/// window keeps decoding memory small for every reader; Codex's decoder
/// accepts windows up to 2^27. The frame carries its content size, which
/// Codex reads from the header for byte-bound checks, and a checksum.
pub struct ZstdRolloutCodec;

const BUFFER_BYTES: usize = 64 * 1024;
/// A zstd frame header occupies at most 18 bytes.
const MAX_FRAME_HEADER_BYTES: usize = 18;

impl ZstdRolloutCodec {
    pub const LEVEL: i32 = 3;
    pub const WINDOW_LOG: u32 = 24;
    /// Codex decodes with the library default limit, which is 2^27.
    pub const MAX_DECODE_WINDOW_LOG: u32 = 27;

    /// Compresses exactly `source_len` bytes of `source` into `output` and
    /// returns the digest of the bytes read. A source that ends early or
    /// keeps going fails, so a concurrently changed file is never published.
    pub fn encode(
        source: &mut impl Read,
        source_len: u64,
        output: &mut impl Write,
    ) -> io::Result<ContentDigest> {
        let mut encoder = zstd::stream::write::Encoder::new(output, Self::LEVEL)?;
        encoder.set_parameter(CParameter::WindowLog(Self::WINDOW_LOG))?;
        encoder.set_parameter(CParameter::EnableLongDistanceMatching(true))?;
        encoder.include_checksum(true)?;
        encoder.include_contentsize(true)?;
        encoder.set_pledged_src_size(Some(source_len))?;
        let digest = ContentDigest::copy(&mut source.take(source_len), &mut encoder)?;
        if digest.len != source_len || Self::has_more(source)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "rollout length changed while it was being compressed",
            ));
        }
        encoder.finish()?;
        Ok(digest)
    }

    /// Decodes a whole frame into `output`, returning the digest of the
    /// decoded bytes. The frame checksum, when present, is verified.
    pub fn decode(input: impl Read, output: &mut impl Write) -> io::Result<ContentDigest> {
        let mut decoder = zstd::stream::read::Decoder::new(input)?;
        decoder.window_log_max(Self::MAX_DECODE_WINDOW_LOG)?;
        ContentDigest::copy(&mut decoder, output)
    }

    /// The decoded length recorded in the first frame's header, if any.
    pub fn frame_content_size(input: &mut impl Read) -> io::Result<Option<u64>> {
        let mut header = [0_u8; MAX_FRAME_HEADER_BYTES];
        let mut read = 0;
        while read < header.len() {
            match input.read(&mut header[read..])? {
                0 => break,
                n => read += n,
            }
        }
        Ok(zstd::zstd_safe::get_frame_content_size(&header[..read])
            .ok()
            .flatten())
    }

    fn has_more(source: &mut impl Read) -> io::Result<bool> {
        let mut probe = [0_u8; 1];
        loop {
            match source.read(&mut probe) {
                Ok(read) => return Ok(read > 0),
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(err),
            }
        }
    }
}

/// Buffer size shared by the codec's streaming copies.
pub(super) const fn buffer_bytes() -> usize {
    BUFFER_BYTES
}

#[cfg(test)]
#[path = "zstd_rollout_codec.test.rs"]
mod tests;
