use super::primary_target_selection::prioritize_primary_if_user;
use super::{clean_thread_id, detect_in_progress_threads};

/// Tasks an account switch resumes after it restarts Desktop: every
/// in-progress user task, with the task that asked for the switch first.
pub(super) struct SwitchRestartTargetService;

impl SwitchRestartTargetService {
    pub(super) fn detect() -> Vec<String> {
        let mut threads = detect_in_progress_threads();
        if let Ok(primary) =
            std::env::var("CODEX_PRIMARY_THREAD").or_else(|_| std::env::var("CODEX_THREAD_ID"))
        {
            let primary = clean_thread_id(&primary);
            let _ = prioritize_primary_if_user(
                &crate::storage::codex_home(),
                &mut threads,
                Some(&primary),
            );
        }
        if !threads.is_empty() {
            crate::runtime_print!(
                "📋 Detected {} active in-progress thread(s) before restart: {:?}",
                threads.len(),
                threads
            );
        }
        threads
    }
}
