#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

/// The Codex data directory: `CODEX_HOME` when set, otherwise the live
/// `~/.codex` shared with ChatGPT.app.
pub fn codex_home() -> PathBuf {
    let configured = configured_codex_home(std::env::var("CODEX_HOME").ok());
    #[cfg(test)]
    if let Err(message) = check_test_home(
        configured.as_deref(),
        super::test_codex_home::active_home().map(|(path, _)| path),
    ) {
        panic!("{message}");
    }
    configured.unwrap_or_else(|| live_codex_home(dirs::home_dir()))
}

/// An unset or empty `CODEX_HOME` selects the live home.
fn configured_codex_home(value: Option<String>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

fn live_codex_home(user_home: Option<PathBuf>) -> PathBuf {
    user_home.map_or_else(|| PathBuf::from(".codex"), |home| home.join(".codex"))
}

/// Test builds resolve only the home of the currently held `TestCodexHome`.
/// Anything else is the live Desktop home, a value inherited from the
/// developer's shell, or a leftover, and each would let a test touch state it
/// does not own. The owning thread is not checked: production code resolves
/// the home on worker threads it spawns for the guarded test.
#[cfg(test)]
fn check_test_home(configured: Option<&Path>, active: Option<PathBuf>) -> Result<(), String> {
    const HINT: &str = "hold a storage::test_codex_home::TestCodexHome";
    let Some(active_path) = active else {
        return Err(format!("no test owns CODEX_HOME; {HINT}"));
    };
    match configured {
        None => Err(format!("CODEX_HOME is unset; {HINT}")),
        Some(path) if path != active_path => Err(format!(
            "CODEX_HOME {} is not the guard's home {}; {HINT}",
            path.display(),
            active_path.display()
        )),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
#[path = "codex_home_resolver.test.rs"]
mod tests;
