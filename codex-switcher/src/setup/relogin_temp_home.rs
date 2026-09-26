use std::{
    fmt::Write as _,
    fs::DirBuilder,
    io::ErrorKind,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

/// A private CODEX_HOME for the browser OAuth command. The directory is
/// created atomically with a random name and never reuses an existing path.
pub(super) struct ReloginTempHome {
    path: PathBuf,
}

impl ReloginTempHome {
    pub(super) fn new() -> Result<Self, String> {
        for _ in 0..4 {
            let mut nonce = [0_u8; 16];
            getrandom::getrandom(&mut nonce)
                .map_err(|_| "Could not generate a private login directory name".to_string())?;
            let mut suffix = String::with_capacity(nonce.len() * 2);
            for byte in nonce {
                write!(&mut suffix, "{byte:02x}")
                    .map_err(|_| "Could not name the private login directory".to_string())?;
            }
            let path = std::env::temp_dir().join(format!("codex-relogin-{suffix}"));
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("Could not create private login directory: {error}"))
                }
            }
        }
        Err("Could not allocate a unique private login directory".into())
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ReloginTempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
