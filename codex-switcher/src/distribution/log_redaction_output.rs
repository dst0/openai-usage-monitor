use super::log_redaction_service::LogRedactionService;

impl LogRedactionService {
    pub fn background_text(value: &str) -> String {
        Self::sanitize_text(value)
    }

    pub fn print_background(value: &str) {
        println!("{}", Self::background_text(value));
    }

    pub fn eprint_background(value: &str) {
        eprintln!("{}", Self::background_text(value));
    }

    pub fn print_runtime(value: &str) {
        if Self::is_background_process() {
            Self::print_background(value);
        } else {
            println!("{value}");
        }
    }

    pub fn eprint_runtime(value: &str) {
        if Self::is_background_process() {
            Self::eprint_background(value);
        } else {
            eprintln!("{value}");
        }
    }

    fn is_background_process() -> bool {
        std::env::var_os("CODEX_RESTART_WORKER").is_some()
            || std::env::var_os("CODEX_MONITOR_BACKGROUND").is_some()
    }
}

#[macro_export]
macro_rules! runtime_print {
    ($($arg:tt)*) => {{
        $crate::distribution::LogRedactionService::print_runtime(&format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! runtime_error {
    ($($arg:tt)*) => {{
        $crate::distribution::LogRedactionService::eprint_runtime(&format!($($arg)*));
    }};
}
