use crate::distribution::WindowProcessValidationService;
use crate::window_action::WindowAction;
use crate::{distribution::SystemWindowRestoreBackend, recovery, switcher};

pub(super) struct DesktopCommandService;

impl DesktopCommandService {
    pub(super) fn window(action: Option<WindowAction>) -> Result<(), String> {
        match action.unwrap_or(WindowAction::Status) {
            WindowAction::Status => {
                let saved = recovery::get_saved_desktop_window_bounds()?;
                if let Some(bounds) = saved {
                    println!(
                        "💾 Saved window bounds: x={:.1}, y={:.1}, w={:.1}, h={:.1} (updated: {})",
                        bounds.x, bounds.y, bounds.width, bounds.height, bounds.updated_at
                    );
                } else {
                    println!("💾 Saved window bounds: None");
                }
                match recovery::get_active_desktop_window_bounds() {
                    Ok(active) => println!(
                        "🖥️  Active window bounds: x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                        active.x, active.y, active.width, active.height
                    ),
                    Err(error) => println!("🖥️  Active window bounds: Unavailable ({error})"),
                }
                println!(
                    "⚙️  preserve_window_bounds_on_restart: {}",
                    recovery::should_preserve_window_bounds()
                );
                Ok(())
            }
            WindowAction::Save => {
                if let Some(bounds) = recovery::save_desktop_window_bounds(None)? {
                    println!(
                        "✅ Window bounds saved: x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                        bounds.x, bounds.y, bounds.width, bounds.height
                    );
                } else {
                    println!(
                        "⚠️ Could not save window bounds (window not found or helper unavailable)"
                    );
                }
                Ok(())
            }
            WindowAction::Restore => {
                let pids = switcher::current_codex_app_pids();
                if pids.is_empty() {
                    return Err("Codex Desktop app is not running".into());
                }
                recovery::restore_desktop_window_bounds(pids[0])?;
                println!("✅ Window bounds restore dispatched for PID {}", pids[0]);
                Ok(())
            }
            WindowAction::ProbeTasks {
                allow_focus_and_clipboard,
            } => {
                if !allow_focus_and_clipboard {
                    return Err("Explicit --allow-focus-and-clipboard is required".into());
                }
                let pids = switcher::current_codex_app_pids_checked()?;
                if pids.len() != 1 {
                    return Err("Task probe requires exactly one ChatGPT main process".into());
                }
                let mut backend = SystemWindowRestoreBackend::new()?;
                let process = WindowProcessValidationService::inspect(&mut backend, pids[0])?;
                let count = backend.probe_selected_tasks(process)?;
                println!("Observed {count} distinct task links while probing ChatGPT windows; clipboard source attribution is unverified. Clipboard may contain the last copied link; no restart was performed.");
                Ok(())
            }
        }
    }
}
