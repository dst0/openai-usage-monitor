use std::io::{ErrorKind, Read};
use std::path::Path;

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
    pub(super) fn verify_default(codex_home: &Path) -> Result<(), String> {
        let file = match std::fs::File::open(codex_home.join(KEYMAP_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(UNREADABLE_KEYMAP.into()),
        };
        let mut text = String::new();
        file.take(MAX_KEYMAP_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|_| UNREADABLE_KEYMAP.to_string())?;
        if text.len() as u64 > MAX_KEYMAP_BYTES {
            return Err(CUSTOM_KEYMAP.into());
        }
        if text.trim().is_empty() {
            return Ok(());
        }
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Array(bindings)) if bindings.is_empty() => Ok(()),
            _ => Err(CUSTOM_KEYMAP.into()),
        }
    }
}

#[cfg(test)]
#[path = "copy_deeplink_keymap_service.test.rs"]
mod tests;
