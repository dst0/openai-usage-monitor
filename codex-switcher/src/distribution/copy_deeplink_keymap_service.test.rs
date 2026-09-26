use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// A private temporary Codex home; never the live `~/.codex`.
struct KeymapHome(PathBuf);

impl KeymapHome {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "codex-keymap-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, contents: impl AsRef<[u8]>) -> &Path {
        std::fs::write(self.0.join(KEYMAP_FILE), contents).unwrap();
        &self.0
    }
}

impl Drop for KeymapHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn default_keymap_is_absent_blank_or_an_empty_array() {
    let home = KeymapHome::new("default");
    assert_eq!(CopyDeeplinkKeymapService::verify_default(&home.0), Ok(None));
    assert_eq!(
        CopyDeeplinkKeymapService::verify_default(&home.0.join("missing-home")),
        Ok(None)
    );
    for contents in ["", " \n\t", "[]", " [ ]\n"] {
        let accepted = CopyDeeplinkKeymapService::verify_default(home.write(contents));
        let modified = std::fs::metadata(home.0.join(KEYMAP_FILE))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(accepted, Ok(Some(modified)), "{contents:?}");
    }
    // Exactly at the size limit is still inspected.
    let padded = format!("[]{}", " ".repeat(MAX_KEYMAP_BYTES as usize - 2));
    assert!(CopyDeeplinkKeymapService::verify_default(home.write(padded)).is_ok());
}

#[test]
fn an_edit_changes_the_returned_modification_time() {
    let home = KeymapHome::new("edit");
    let path = home.write("[]").join(KEYMAP_FILE);
    let before = CopyDeeplinkKeymapService::verify_default(&home.0).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let after = CopyDeeplinkKeymapService::verify_default(&home.0).unwrap();
    assert!(before.is_some() && after == Some(later) && after != before);
}

#[test]
fn any_override_or_unparsed_keymap_refuses_the_probe() {
    let home = KeymapHome::new("custom");
    for contents in [
        r#"[{"command":"archiveThread","key":"CmdOrCtrl+Alt+L"}]"#,
        r#"[{"command":"copyDeeplink","key":"CmdOrCtrl+Alt+Shift+L"}]"#,
        r#"[{"command":"copyDeeplink","key":null}]"#,
        "[{}]",
        "{}",
        "null",
        "[] // comment",
        "[",
        "\u{feff}[]",
    ] {
        assert_eq!(
            CopyDeeplinkKeymapService::verify_default(home.write(contents)),
            Err(CUSTOM_KEYMAP.into()),
            "{contents:?}"
        );
    }
    let oversized = format!("[]{}", " ".repeat(MAX_KEYMAP_BYTES as usize - 1));
    assert_eq!(
        CopyDeeplinkKeymapService::verify_default(home.write(oversized)),
        Err(CUSTOM_KEYMAP.into())
    );
}

#[test]
fn unreadable_keymap_refuses_the_probe() {
    let home = KeymapHome::new("unreadable");
    assert_eq!(
        CopyDeeplinkKeymapService::verify_default(home.write([b'[', 0xff, b']'])),
        Err(UNREADABLE_KEYMAP.into())
    );
    std::fs::remove_file(home.0.join(KEYMAP_FILE)).unwrap();
    std::fs::create_dir(home.0.join(KEYMAP_FILE)).unwrap();
    assert_eq!(
        CopyDeeplinkKeymapService::verify_default(&home.0),
        Err(UNREADABLE_KEYMAP.into())
    );
    std::fs::remove_dir(home.0.join(KEYMAP_FILE)).unwrap();
    // A FIFO would block a blocking open until a writer appears.
    let fifo = std::ffi::CString::new(
        home.0
            .join(KEYMAP_FILE)
            .into_os_string()
            .into_string()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert_eq!(
        CopyDeeplinkKeymapService::verify_default(&home.0),
        Err(UNREADABLE_KEYMAP.into())
    );
    std::fs::remove_file(home.0.join(KEYMAP_FILE)).unwrap();
    let path = home.write("[]").join(KEYMAP_FILE);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root can read a mode-000 file; the refusal is only observable otherwise.
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!(
            CopyDeeplinkKeymapService::verify_default(&home.0),
            Err(UNREADABLE_KEYMAP.into())
        );
    }
}

#[test]
fn keymap_is_read_through_a_symlink_like_chatgpt_does() {
    let home = KeymapHome::new("symlink");
    let target = home.0.join("real-keymap.json");
    std::fs::write(
        &target,
        r#"[{"command":"archiveThread","key":"CmdOrCtrl+Alt+L"}]"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&target, home.0.join(KEYMAP_FILE)).unwrap();
    assert_eq!(
        CopyDeeplinkKeymapService::verify_default(&home.0),
        Err(CUSTOM_KEYMAP.into())
    );
    std::fs::write(&target, "[]").unwrap();
    assert!(CopyDeeplinkKeymapService::verify_default(&home.0)
        .unwrap()
        .is_some());
    // A dangling link is what ChatGPT also reads as "no overrides".
    std::fs::remove_file(&target).unwrap();
    assert_eq!(CopyDeeplinkKeymapService::verify_default(&home.0), Ok(None));
}
