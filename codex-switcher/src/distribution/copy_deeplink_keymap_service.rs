use std::io::{ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::SystemTime;

/// ChatGPT reads command keybinding overrides from this file in its Codex home.
const KEYMAP_FILE: &str = "keybindings.json";
/// A larger keymap is not inspected; the probe refuses it instead.
const MAX_KEYMAP_BYTES: u64 = 64 * 1024;
const CUSTOM_KEYMAP: &str =
    "ChatGPT keybindings rebind Copy deeplink or the L key; the task probe runs only while Copy deeplink keeps its default Cmd+Opt+L";
const UNREADABLE_KEYMAP: &str = "ChatGPT keybindings could not be read; refusing the task probe";

/// The command whose default shortcut the probe synthesizes.
const COPY_DEEPLINK_COMMAND: &str = "copyDeeplink";
/// The only fields ChatGPT reads from a keymap entry.
const ENTRY_FIELDS: [&str; 2] = ["command", "key"];

/// Proves the probe's synthesized Cmd+Opt+L still means Copy deeplink.
/// ChatGPT 26.924.22138 binds its `copyDeeplink` command to CmdOrCtrl+Alt+L
/// by default (no other default uses that chord) and applies overrides from
/// `$CODEX_HOME/keybindings.json`, an array of `{command, key}` entries. A
/// command named by any entry uses exactly those keys; every other command
/// keeps its defaults, and no legacy alias maps to `copyDeeplink`. So the
/// shortcut is unchanged exactly when no entry names `copyDeeplink` and no
/// entry binds the L key. This service accepts only such keymaps, refusing
/// every entry that names Copy deeplink, puts any modifier combination on L
/// in any chord step (or a key ChatGPT might translate to L), or does not
/// have exactly ChatGPT's two fields and types.
pub(super) struct CopyDeeplinkKeymapService;

impl CopyDeeplinkKeymapService {
    /// Returns the accepted keymap's modification time (`None` when absent),
    /// so a caller can detect an edit made while it relied on the keymap.
    pub(super) fn verify_copy_binding(codex_home: &Path) -> Result<Option<SystemTime>, String> {
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
            Ok(serde_json::Value::Array(bindings))
                if bindings.iter().all(Self::entry_leaves_copy_shortcut) =>
            {
                Ok(Some(modified))
            }
            _ => Err(CUSTOM_KEYMAP.into()),
        }
    }

    /// True only for an entry with exactly ChatGPT's fields and types that
    /// neither names Copy deeplink nor binds the L key.
    fn entry_leaves_copy_shortcut(entry: &serde_json::Value) -> bool {
        let Some(fields) = entry.as_object() else {
            return false;
        };
        if fields.len() != ENTRY_FIELDS.len()
            || !ENTRY_FIELDS.iter().all(|field| fields.contains_key(*field))
        {
            return false;
        }
        let Some(command) = fields["command"].as_str() else {
            return false;
        };
        if command == COPY_DEEPLINK_COMMAND {
            return false;
        }
        match &fields["key"] {
            serde_json::Value::Null => true,
            serde_json::Value::String(key) => !Self::binds_l_key(key),
            _ => false,
        }
    }

    /// Checks every token of every chord step: ChatGPT compares only the
    /// first step of a menu accelerator, but its renderer may handle later
    /// ones, and an accelerator may name its key before a modifier. No
    /// modifier is spelled `l`. A non-ASCII token could be the character
    /// Option+L types, so it counts as L too.
    fn binds_l_key(key: &str) -> bool {
        key.split_whitespace()
            .flat_map(|step| step.split('+'))
            .any(|token| {
                !token.is_ascii()
                    || token.eq_ignore_ascii_case("l")
                    || token.eq_ignore_ascii_case("keyl")
            })
    }
}

#[cfg(test)]
#[path = "copy_deeplink_keymap_service.test.rs"]
mod tests;
