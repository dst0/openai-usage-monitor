use serde::{Deserialize, Serialize};

/// A PID paired with a kernel birth token so a recycled PID cannot be trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub birth_identity: String,
}

impl ProcessIdentity {
    pub fn new(pid: u32, birth_identity: impl Into<String>) -> Result<Self, String> {
        let birth_identity = birth_identity.into();
        if pid <= 1 {
            return Err("Codex process identity requires a valid PID".into());
        }
        if birth_identity.trim().is_empty()
            || birth_identity.len() > 128
            || birth_identity.chars().any(|ch| ch.is_control())
        {
            return Err("Codex process identity has an invalid birth token".into());
        }
        Ok(Self {
            pid,
            birth_identity,
        })
    }

    #[cfg(target_os = "macos")]
    pub fn from_kernel(pid: u32) -> Result<Self, String> {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let expected_size = std::mem::size_of::<libc::proc_bsdinfo>();
        // SAFETY: libproc writes exactly the requested structure size before the
        // value is read. The PID is caller supplied but proc_pidinfo validates it.
        let written = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                expected_size as libc::c_int,
            )
        };
        if written != expected_size as libc::c_int {
            return Err("Could not read Codex process birth identity".into());
        }
        // SAFETY: the exact-size return above establishes full initialization.
        let info = unsafe { info.assume_init() };
        Self::new(
            pid,
            format!("{}:{:06}", info.pbi_start_tvsec, info.pbi_start_tvusec),
        )
    }

    #[cfg(target_os = "linux")]
    pub fn from_kernel(pid: u32) -> Result<Self, String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_err(|_| "Could not read Codex process birth identity".to_string())?;
        let after_name = stat.rsplit_once(") ").ok_or("Malformed process status")?.1;
        let ticks = after_name
            .split_whitespace()
            .nth(19)
            .ok_or("Process status has no birth token")?;
        Self::new(pid, ticks)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub fn from_kernel(_pid: u32) -> Result<Self, String> {
        Err("Kernel process birth identity is unsupported on this platform".into())
    }
}
