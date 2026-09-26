use std::io::{ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::SystemTime;

/// ChatGPT reads command keybinding overrides from this file in its Codex home.
const KEYMAP_FILE: &str = "keybindings.json";
/// A larger keymap is not inspected; the probe refuses it instead.
const MAX_KEYMAP_BYTES: u64 = 64 * 1024;
const CUSTOM_KEYMAP: &str =
    "ChatGPT keybindings are not the default empty keymap; the task probe runs only with the default Copy deeplink shortcut";
const UNREADABLE_KEYMAP: &str = "ChatGPT keybindings could not be read; refusing the task probe";

/// Proves the probe's synthesized Cmd+Opt+L still means Copy deeplink.
/// ChatGPT 26.924.20706 binds its `copyDeeplink` command to CmdOrCtrl+Alt+L
/// by default and applies overrides from `$CODEX_HOME/keybindings.json`. An
/// override can move that shortcut to another command, such as archiving the
/// task, so only an absent, blank, or empty-array keymap is accepted.
pub(super) struct CopyDeeplinkKeymapService;

impl CopyDeeplinkKeymapService {
    /// Returns the accepted keymap's modification time (`None` when absent),
    /// so a caller can detect an edit made while it relied on the keymap.
    pub(super) fn verify_default(codex_home: &Path) -> Result<Option<SystemTime>, String> {
        // Non-blocking, so a FIFO cannot stall the caller while it holds the
        // operation lock; only a regular file is then read.
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(codex_home.join(KEYMAP_FILE))
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(UNREADABLE_KEYMAP.into()),
        };
        let metadata = file.metadata().map_err(|_| UNREADABLE_KEYMAP.to_string())?;
        if !metadata.is_file() {
            return Err(UNREADABLE_KEYMAP.into());
        }
        let modified = metadata
            .modified()
            .map_err(|_| UNREADABLE_KEYMAP.to_string())?;
        let mut text = String::new();
        file.take(MAX_KEYMAP_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|_| UNREADABLE_KEYMAP.to_string())?;
        if text.len() as u64 > MAX_KEYMAP_BYTES {
            return Err(CUSTOM_KEYMAP.into());
        }
        if text.trim().is_empty() {
            return Ok(Some(modified));
        }
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Array(bindings)) if bindings.is_empty() => Ok(Some(modified)),
            _ => Err(CUSTOM_KEYMAP.into()),
        }
    }
}

#[cfg(test)]
#[path = "copy_deeplink_keymap_service.test.rs"]
mod tests;
