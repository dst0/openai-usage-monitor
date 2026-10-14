use std::io::{self, Read, Write};

/// Length and SHA-256 of a rollout's original bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentDigest {
    pub len: u64,
    pub sha256: [u8; 32],
}

impl ContentDigest {
    /// Streams `reader` into `writer` and returns the digest of what passed.
    pub fn copy(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<Self> {
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buffer = vec![0_u8; super::zstd_rollout_codec::buffer_bytes()];
        let mut len = 0_u64;
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            };
            context.update(&buffer[..read]);
            writer.write_all(&buffer[..read])?;
            len += read as u64;
        }
        writer.flush()?;
        let mut sha256 = [0_u8; 32];
        sha256.copy_from_slice(context.finish().as_ref());
        Ok(Self { len, sha256 })
    }

    /// The digest of `reader`'s remaining bytes.
    pub fn of(reader: &mut impl Read) -> io::Result<Self> {
        Self::copy(reader, &mut io::sink())
    }
}
