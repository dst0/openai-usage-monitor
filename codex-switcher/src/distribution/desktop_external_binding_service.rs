use super::daemon_account_sync_service::DaemonAccountSyncService;
use super::desktop_app_session::DesktopAppSession;
use super::window_restore_process_identity::ProcessIdentity;
use super::{SystemWindowRestoreBackend, WindowProcessValidationService};
use crate::models::{AccountsFile, AuthJson};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::de::DeserializeOwned;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{Read, Take};
use std::os::macos::fs::MetadataExt as MacMetadataExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_IDENTITY_FILE_BYTES: u64 = 1024 * 1024;

/// Plans a conservative binding for a Desktop launched outside the switcher.
/// The caller must also verify stable file and process identities around save.
pub(super) struct DesktopExternalBindingService;

impl DesktopExternalBindingService {
    pub(super) fn refresh_if_needed() -> Result<bool, String> {
        let _operation = match crate::recovery::operation_lock() {
            Ok(lock) => lock,
            Err(_) => return Ok(false),
        };
        let home = crate::storage::codex_home();
        if DesktopAppSession::load_checked(&home.join("desktop-app-session.json"))?.is_none()
            || crate::switcher::current_codex_app_pids().len() != 1
        {
            return Ok(false);
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        Self::refresh_with(
            &home,
            SystemTime::now(),
            crate::switcher::current_codex_app_pids,
            |pid| WindowProcessValidationService::inspect(&mut backend, pid),
            || Self::read_auth_evidence(&home),
        )
    }

    fn refresh_with(
        home: &Path,
        now: SystemTime,
        mut pids: impl FnMut() -> Vec<u32>,
        mut inspect: impl FnMut(u32) -> Result<ProcessIdentity, String>,
        mut auth_evidence: impl FnMut() -> Option<(String, String, SystemTime, SystemTime)>,
    ) -> Result<bool, String> {
        let marker_path = home.join("desktop-app-session.json");
        let Some(previous) = DesktopAppSession::load_checked(&marker_path)? else {
            return Ok(false);
        };
        let current_pids = pids();
        if current_pids.len() != 1 {
            return Ok(false);
        }
        let first_process = inspect(current_pids[0])?;
        let Some((account_id, auth_file_id, auth_modified, auth_created)) = auth_evidence() else {
            return Ok(false);
        };
        let Some(candidate) = Self::candidate(
            &previous,
            &first_process,
            &account_id,
            &auth_file_id,
            auth_modified,
            auth_created,
            now,
        ) else {
            return Ok(false);
        };
        if pids() != current_pids
            || inspect(current_pids[0])? != first_process
            || auth_evidence()
                != Some((
                    account_id.clone(),
                    auth_file_id.clone(),
                    auth_modified,
                    auth_created,
                ))
            || DesktopAppSession::load_checked(&marker_path)? != Some(previous.clone())
        {
            return Ok(false);
        }
        if let Err(error) = candidate.save(&marker_path) {
            DesktopAppSession::restore_after_failed_save(
                &marker_path,
                Some(&previous),
                &candidate,
            )?;
            return Err(error);
        }
        if pids() == current_pids
            && inspect(current_pids[0]).ok().as_ref() == Some(&first_process)
            && auth_evidence() == Some((account_id, auth_file_id, auth_modified, auth_created))
        {
            return Ok(true);
        }
        // Never leave an externally inferred binding active after evidence changes.
        DesktopAppSession::restore_after_failed_save(&marker_path, Some(&previous), &candidate)?;
        Ok(false)
    }

    pub(super) fn read_auth_evidence(
        home: &Path,
    ) -> Option<(String, String, SystemTime, SystemTime)> {
        let (auth, auth_meta): (AuthJson, Metadata) = read_private_json(&home.join("auth.json"))?;
        let (accounts, _): (AccountsFile, Metadata) =
            read_private_json(&home.join("accounts.json"))?;
        if !DaemonAccountSyncService::active_auth_matches_registry(&accounts, &auth) {
            return None;
        }
        if auth.auth_mode.as_deref() != Some("chatgpt") {
            return None;
        }
        let tokens = auth.tokens.as_ref()?;
        let provider_id = tokens.account_id.as_deref()?.trim();
        if provider_id.is_empty() || tokens.access_token.trim().is_empty() {
            return None;
        }
        let mut matching = accounts.accounts.iter().filter(|account| {
            account.account_id == provider_id && account.tokens == *tokens && account.enabled
        });
        let account = matching.next()?;
        if matching.next().is_some() || accounts.active_account_id.as_deref() != Some(&account.id) {
            return None;
        }
        Some((
            account.id.clone(),
            auth_file_id(&auth_meta),
            auth_meta.modified().ok()?,
            metadata_provenance_time(&auth_meta)?,
        ))
    }

    pub(super) fn candidate(
        previous: &DesktopAppSession,
        current: &ProcessIdentity,
        authenticated_account_id: &str,
        auth_file_id: &str,
        auth_modified: SystemTime,
        auth_created: SystemTime,
        now: SystemTime,
    ) -> Option<DesktopAppSession> {
        let old_process = previous.process.as_ref()?;
        let old_birth = process_birth(&old_process.birth_id)?;
        let current_birth = process_birth(&current.birth_id)?;
        let old_saved = DateTime::parse_from_rfc3339(&previous.updated_at)
            .ok()?
            .with_timezone(&Utc);
        let auth_modified: DateTime<Utc> = auth_modified.into();
        let auth_created: DateTime<Utc> = auth_created.into();
        let now: DateTime<Utc> = now.into();
        if authenticated_account_id.trim().is_empty()
            || auth_file_id.is_empty()
            || previous.account_id != authenticated_account_id
            || old_process == current
            || old_birth >= current_birth
            || old_saved < old_birth
            || old_saved >= current_birth
            || auth_modified >= current_birth
            || auth_created >= current_birth
            || now < current_birth
        {
            return None;
        }
        let mut candidate = DesktopAppSession::bound(
            authenticated_account_id,
            authenticated_account_id,
            current.clone(),
        );
        candidate.updated_at = now.to_rfc3339();
        candidate.auth_file_id = Some(auth_file_id.into());
        Some(candidate)
    }
}

fn metadata_provenance_time(meta: &Metadata) -> Option<SystemTime> {
    let timestamp = |seconds: i64, nanos: i64| {
        let seconds = u64::try_from(seconds).ok()?;
        let nanos = u32::try_from(nanos).ok()?;
        (nanos < 1_000_000_000).then_some(())?;
        UNIX_EPOCH.checked_add(Duration::new(seconds, nanos))
    };
    // macOS may backdate birthtime when utimes restores an older mtime; ctime
    // still records that the file changed after the Desktop process started.
    Some(
        timestamp(meta.st_birthtime(), meta.st_birthtime_nsec())?
            .max(timestamp(meta.ctime(), meta.ctime_nsec())?),
    )
}

pub(super) fn read_private_json<T: DeserializeOwned>(path: &Path) -> Option<(T, Metadata)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    FileExt::try_lock_shared(&file).ok()?;
    let opened = file.metadata().ok()?;
    // SAFETY: geteuid has no pointer arguments and only reads this process's UID.
    let uid = unsafe { libc::geteuid() };
    if !opened.is_file()
        || opened.uid() != uid
        || opened.permissions().mode() & 0o777 != 0o600
        || opened.len() > MAX_IDENTITY_FILE_BYTES
    {
        return None;
    }
    let mut bytes = Vec::new();
    let mut limited: Take<&mut std::fs::File> = (&mut file).take(MAX_IDENTITY_FILE_BYTES + 1);
    limited.read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 != opened.len() {
        return None;
    }
    let observed = file.metadata().ok()?;
    let named = fs::symlink_metadata(path).ok()?;
    if !named.is_file()
        || named.uid() != uid
        || named.permissions().mode() & 0o777 != 0o600
        || named.dev() != opened.dev()
        || named.ino() != opened.ino()
        || named.mtime() != opened.mtime()
        || named.mtime_nsec() != opened.mtime_nsec()
        || named.ctime() != opened.ctime()
        || named.ctime_nsec() != opened.ctime_nsec()
        || named.len() != opened.len()
        || observed.dev() != opened.dev()
        || observed.ino() != opened.ino()
        || observed.mtime() != opened.mtime()
        || observed.mtime_nsec() != opened.mtime_nsec()
        || observed.ctime() != opened.ctime()
        || observed.ctime_nsec() != opened.ctime_nsec()
        || observed.len() != opened.len()
    {
        return None;
    }
    Some((serde_json::from_slice(&bytes).ok()?, opened))
}

pub(super) fn auth_file_id(meta: &Metadata) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        meta.dev(),
        meta.ino(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.len()
    )
}

fn process_birth(raw: &str) -> Option<DateTime<Utc>> {
    let (seconds, micros) = raw.split_once(':')?;
    let seconds = seconds.parse().ok()?;
    let micros: u32 = micros.parse().ok()?;
    if micros > 999_999 {
        return None;
    }
    DateTime::from_timestamp(seconds, micros * 1_000)
}

#[cfg(test)]
#[path = "desktop_external_binding_service.test.rs"]
mod tests;
