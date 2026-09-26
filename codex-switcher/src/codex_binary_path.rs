use std::{
    env,
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub const CODEX_DESKTOP_APP: &str = "/Applications/ChatGPT.app";

pub fn resolve_real_codex_bin() -> Result<PathBuf, String> {
    resolve_real_codex_bin_avoiding(None)
}

/// During shim installation, do not select the very pathname being replaced.
pub fn resolve_real_codex_bin_avoiding(replaced: Option<&Path>) -> Result<PathBuf, String> {
    let dirs = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let self_exe = env::current_exe().map_err(|error| error.to_string())?;
    resolve_real_codex_bin_with(Path::new(CODEX_DESKTOP_APP), &dirs, &self_exe, replaced)
}

fn resolve_real_codex_bin_with(
    app: &Path,
    path_dirs: &[PathBuf],
    self_exe: &Path,
    replaced: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Ok(path) = resolve_real_codex_bin_in(app) {
        return Ok(path);
    }
    let self_exe = self_exe.canonicalize().map_err(|error| error.to_string())?;
    let replaced_parent = replaced
        .and_then(Path::parent)
        .and_then(|parent| parent.canonicalize().ok());
    let replaced_target = replaced.and_then(|path| path.canonicalize().ok());
    for dir in path_dirs {
        let candidate = dir.join("codex");
        // PATH can name the same directory through a symlink. Never rely on
        // the entry that installation is about to replace.
        if replaced_parent
            .as_ref()
            .is_some_and(|parent| dir.canonicalize().ok().as_ref() == Some(parent))
        {
            continue;
        }
        if !executable_file(&candidate, true) {
            continue;
        }
        let Ok(resolved) = candidate.canonicalize() else {
            continue;
        };
        if resolved == self_exe || replaced_target.as_ref() == Some(&resolved) {
            continue;
        }
        // An absolute target also survives exec from another working
        // directory and a relative PATH entry used during shim installation.
        return Ok(resolved);
    }
    Err("No usable Codex CLI found in the Desktop bundle or PATH".into())
}

pub fn resolve_real_codex_bin_in(app: &Path) -> Result<PathBuf, String> {
    let modern = app.join("Contents/Resources/codex-cli/bin/codex");
    let modern_payload = app.join("Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex");
    if executable_file(&modern, false) && executable_file(&modern_payload, false) {
        return Ok(modern);
    }
    let legacy = app.join("Contents/Resources/codex");
    if executable_file(&legacy, false) {
        return Ok(legacy);
    }
    Err("Official Codex Desktop CLI is unavailable".into())
}

fn executable_file(path: &Path, allow_symlink: bool) -> bool {
    let metadata = if allow_symlink {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    };
    metadata.is_ok_and(|metadata| metadata.is_file())
        && CString::new(path.as_os_str().as_bytes()).is_ok_and(|path| {
            // Monitor does not run setuid, so access() checks the credentials
            // that exec will use. Any execute bit alone is insufficient.
            unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    fn fixture(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "codex-cli-layout-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    fn executable(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn resolves_new_bundled_cli_layout_before_legacy() {
        let app = fixture("new");
        let modern = app.join("Contents/Resources/codex-cli/bin/codex");
        let payload = app.join("Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex");
        let legacy = app.join("Contents/Resources/codex");
        executable(&modern);
        executable(&payload);
        executable(&legacy);
        assert_eq!(resolve_real_codex_bin_in(&app).unwrap(), modern);
        fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn resolves_legacy_layout_and_rejects_missing_or_nonexecutable_cli() {
        let app = fixture("legacy");
        let legacy = app.join("Contents/Resources/codex");
        assert!(resolve_real_codex_bin_in(&app).is_err());
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"not executable").unwrap();
        assert!(resolve_real_codex_bin_in(&app).is_err());
        fs::set_permissions(&legacy, fs::Permissions::from_mode(0o001)).unwrap();
        assert!(resolve_real_codex_bin_in(&app).is_err());
        fs::set_permissions(&legacy, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(resolve_real_codex_bin_in(&app).unwrap(), legacy);
        fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn missing_modern_payload_and_bundle_symlink_fail_closed() {
        let app = fixture("broken");
        let modern = app.join("Contents/Resources/codex-cli/bin/codex");
        let payload = app.join("Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex");
        let shim = app.join("monitor-shim");
        executable(&modern);
        assert!(resolve_real_codex_bin_in(&app).is_err());
        executable(&payload);
        fs::remove_file(&modern).unwrap();
        executable(&shim);
        std::os::unix::fs::symlink(&shim, &modern).unwrap();
        assert!(resolve_real_codex_bin_in(&app).is_err());
        fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn standalone_cli_fallback_skips_self_and_path_being_replaced() {
        let root = fixture("standalone");
        let app = root.join("absent-app");
        let shim_dir = root.join("shim-bin");
        let standalone_dir = root.join("standalone-bin");
        let shim = root.join("codex-mon");
        executable(&shim);
        fs::create_dir_all(&shim_dir).unwrap();
        std::os::unix::fs::symlink(&shim, shim_dir.join("codex")).unwrap();
        let standalone = standalone_dir.join("codex");
        executable(&standalone);
        let dirs = [shim_dir, standalone_dir];
        assert_eq!(
            resolve_real_codex_bin_with(&app, &dirs, &shim, None).unwrap(),
            standalone
        );
        assert!(resolve_real_codex_bin_with(&app, &dirs, &shim, Some(&standalone)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shim_install_rejects_sole_standalone_target_behind_replaced_symlink() {
        let root = fixture("standalone-link");
        let app = root.join("absent-app");
        let path_dir = root.join("bin");
        let standalone = root.join("standalone-codex");
        let shim = root.join("codex-mon");
        executable(&standalone);
        executable(&shim);
        fs::create_dir_all(&path_dir).unwrap();
        let replaced = path_dir.join("codex");
        std::os::unix::fs::symlink(&standalone, &replaced).unwrap();
        assert!(
            resolve_real_codex_bin_with(&app, &[path_dir.clone()], &shim, Some(&replaced)).is_err()
        );
        assert_eq!(fs::read_link(&replaced).unwrap(), standalone);
        let alias = root.join("bin-alias");
        std::os::unix::fs::symlink(&path_dir, &alias).unwrap();
        assert!(resolve_real_codex_bin_with(&app, &[alias], &shim, Some(&replaced)).is_err());
        assert_eq!(fs::read_link(&replaced).unwrap(), standalone);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shim_install_rejects_file_alias_of_replaced_standalone() {
        let root = fixture("file-alias");
        let app = root.join("absent-app");
        let shim = root.join("codex-mon");
        executable(&shim);
        let replaced = root.join("local-bin/codex");
        executable(&replaced);
        let alias = root.join("other-bin/codex");
        fs::create_dir_all(alias.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&replaced, &alias).unwrap();
        assert!(resolve_real_codex_bin_with(
            &app,
            &[alias.parent().unwrap().to_path_buf()],
            &shim,
            Some(&replaced)
        )
        .is_err());
        assert_eq!(fs::read(&replaced).unwrap(), b"#!/bin/sh\nexit 0\n");
        fs::remove_dir_all(root).unwrap();
    }
}
