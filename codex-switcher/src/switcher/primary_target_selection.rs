use super::is_user_thread;
use std::path::Path;

pub(super) fn prioritize_primary(targets: &mut Vec<String>, primary: Option<&String>) {
    let Some(primary) = primary else { return };
    if let Some(index) = targets.iter().position(|id| id == primary) {
        targets.remove(index);
    }
    targets.insert(0, primary.clone());
}

pub(super) fn prioritize_primary_if_user(
    codex_home: &Path,
    targets: &mut Vec<String>,
    primary: Option<&String>,
) -> bool {
    if primary.is_some_and(|id| !is_user_thread(codex_home, id)) {
        return false;
    }
    prioritize_primary(targets, primary);
    true
}
