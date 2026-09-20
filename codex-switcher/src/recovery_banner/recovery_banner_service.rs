use super::{
    banner_session_status::BannerSessionStatus, process_identity::ProcessIdentity,
    recovery_banner_owner::RecoveryBannerOwner, recovery_banner_payload::RecoveryBannerPayload,
    recovery_session::RecoverySession, saved_window::SavedWindow,
};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Owns one atomic payload lifecycle; the Swift helper owns the display lease.
pub struct RecoveryBannerService {
    payload_path: PathBuf,
    display_lock_path: PathBuf,
    operation_owner: RecoveryBannerOwner,
    payload: Mutex<RecoveryBannerPayload>,
}

impl RecoveryBannerService {
    pub fn begin(
        home: impl AsRef<Path>,
        operation_id: impl Into<String>,
        expected_process: ProcessIdentity,
        saved_window: SavedWindow,
        sessions: Vec<RecoverySession>,
    ) -> Result<Self, String> {
        let recovery_dir = home.as_ref().join("recovery-runs");
        ensure_private_directory(&recovery_dir)?;
        let operation_owner =
            RecoveryBannerOwner::acquire(recovery_dir.join("restore-banner.operation.lock"))?;
        let payload_path = recovery_dir.join("restore-banner.json");
        let display_lock_path = recovery_dir.join("restore-banner.display.lock");
        let payload =
            RecoveryBannerPayload::new(operation_id, expected_process, saved_window, sessions)?;
        write_atomic(&payload_path, &payload)?;
        Ok(Self {
            payload_path,
            display_lock_path,
            operation_owner,
            payload: Mutex::new(payload),
        })
    }

    pub fn payload_path(&self) -> &Path {
        &self.payload_path
    }

    pub fn display_lock_path(&self) -> &Path {
        &self.display_lock_path
    }

    pub fn operation_lock_path(&self) -> &Path {
        self.operation_owner.path()
    }

    pub fn update_sessions(&self, sessions: Vec<RecoverySession>) -> Result<(), String> {
        let mut payload = self.lock_payload()?;
        payload.replace_sessions(sessions);
        write_atomic(&self.payload_path, &payload)
    }

    pub fn update_target(
        &self,
        expected_process: ProcessIdentity,
        saved_window: SavedWindow,
    ) -> Result<(), String> {
        let mut payload = self.lock_payload()?;
        payload.expected_process = expected_process;
        payload.saved_window = saved_window;
        payload.updated_at_unix_ms = now_unix_ms();
        write_atomic(&self.payload_path, &payload)
    }

    pub fn update_status(&self, short_id: &str, status: BannerSessionStatus) -> Result<(), String> {
        let mut payload = self.lock_payload()?;
        payload.update_status(short_id, status)?;
        write_atomic(&self.payload_path, &payload)
    }

    pub fn read_payload(&self) -> Result<RecoveryBannerPayload, String> {
        let payload = self.lock_payload()?;
        Ok(payload.clone())
    }

    pub fn finish(self) -> Result<(), String> {
        remove_payload(&self.payload_path)
    }

    fn lock_payload(&self) -> Result<std::sync::MutexGuard<'_, RecoveryBannerPayload>, String> {
        self.payload
            .lock()
            .map_err(|_| "Recovery banner payload lock is poisoned".to_string())
    }
}

impl Drop for RecoveryBannerService {
    fn drop(&mut self) {
        let _ = remove_payload(&self.payload_path);
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Recovery banner directory must not be a symlink".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    let expected_uid = unsafe { libc::geteuid() };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o077 != 0
    {
        return Err("Recovery banner directory is not private to the current user".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())
}

fn write_atomic(path: &Path, payload: &RecoveryBannerPayload) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Recovery banner payload has no parent".to_string())?;
    ensure_private_directory(parent)?;
    let temporary = parent.join(format!(
        ".restore-banner.{}.{}.tmp",
        std::process::id(),
        payload.updated_at_unix_ms
    ));
    let data = serde_json::to_vec_pretty(payload)
        .map_err(|error| format!("Could not serialize recovery banner payload: {error}"))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|error| format!("Could not create recovery banner payload: {error}"))?;
    let result = (|| {
        file.lock_exclusive()
            .map_err(|error| format!("Could not lock recovery banner payload: {error}"))?;
        file.write_all(&data)
            .map_err(|error| format!("Could not write recovery banner payload: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not sync recovery banner payload: {error}"))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not publish recovery banner payload: {error}"))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_payload(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not remove recovery banner payload: {error}")),
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
