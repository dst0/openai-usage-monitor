use serde::{Deserialize, Serialize};

/// Stable opaque process birth identity emitted by the macOS helper.
///
/// The helper deliberately owns the representation (`seconds:microseconds`).
/// The Rust side must preserve it byte-for-byte instead of converting it to a
/// numeric type, because the two halves of the identity are part of the
/// process-reuse check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub birth_id: String,
}

impl ProcessIdentity {
    pub fn new(pid: u32, birth_id: impl Into<String>) -> Result<Self, String> {
        let birth_id = birth_id.into();
        if pid <= 1 {
            return Err("Process PID is not valid".into());
        }
        if birth_id.is_empty()
            || birth_id.len() > 128
            || birth_id.chars().any(|character| character.is_control())
        {
            return Err("Process birth identity is not valid".into());
        }
        Ok(Self { pid, birth_id })
    }

    pub fn is_for(&self, expected_pid: u32) -> bool {
        self.pid == expected_pid
    }
}
