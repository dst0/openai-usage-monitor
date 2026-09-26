use super::window_restore_backend::WindowRestoreBackend;
use super::window_restore_capture::WindowCapture;
use super::window_restore_frame::WindowFrame;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_restore_screen::ScreenIdentity;
use super::window_task_probe_validation_service::WindowTaskProbeValidationService;
use std::path::PathBuf;
use std::process::Command;

/// macOS backend for the standalone, PID-bound accessibility helper.
pub struct SystemWindowRestoreBackend {
    helper: PathBuf,
}

impl SystemWindowRestoreBackend {
    /// Explicit diagnostic only. The native helper handles focus and clipboard;
    /// Rust validates the counts and exact process without receiving task IDs.
    pub fn probe_selected_tasks(&mut self, process: ProcessIdentity) -> Result<usize, String> {
        let mut args = Self::args_for_process("probe-selected-tasks", process.clone());
        args.extend(["--allow-focus-and-clipboard".into(), "yes".into()]);
        let response = self.invoke(&args)?;
        WindowTaskProbeValidationService::parse(&response, &process)
    }
    /// Counts only WindowServer windows proven to be ChatGPT standard windows.
    /// An unnamed visible window makes the result ambiguous and blocks shutdown.
    pub fn capture_window_inventory(&mut self, process: ProcessIdentity) -> Result<usize, String> {
        let response = self.invoke(&Self::args_for_process(
            "count-standard-windows",
            process.clone(),
        ))?;
        Self::parse_window_inventory(&response, &process)
    }

    fn parse_window_inventory(
        response: &serde_json::Value,
        expected: &ProcessIdentity,
    ) -> Result<usize, String> {
        if Self::parse_process(&response["process"])? != *expected {
            return Err("Window inventory process identity changed".into());
        }
        let ambiguous = response["ambiguous_count"]
            .as_u64()
            .ok_or("Window inventory has no ambiguity count")?;
        if ambiguous != 0 {
            return Err("Window inventory contains an unidentified window".into());
        }
        let ids = response["window_ids"]
            .as_array()
            .ok_or("Window inventory has no window IDs")?;
        let ax_count = response["ax_standard_count"]
            .as_u64()
            .ok_or("Window inventory has no Accessibility window count")?;
        if ax_count != ids.len() as u64 {
            return Err("WindowServer and Accessibility window counts differ".into());
        }
        if ids.len() > 64 {
            return Err("Window inventory exceeds the supported window limit".into());
        }
        let mut unique = std::collections::HashSet::new();
        for id in ids {
            let id = id
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .filter(|id| *id != 0)
                .ok_or("Window inventory contains an invalid window ID")?;
            if !unique.insert(id) {
                return Err("Window inventory contains a duplicate window ID".into());
            }
        }
        Ok(ids.len())
    }

    pub fn capture_banner_window(
        &mut self,
        process: ProcessIdentity,
    ) -> Result<WindowCapture, String> {
        let capture = Self::parse_capture(self.invoke(&Self::args_for_process(
            "capture-banner-window",
            process.clone(),
        ))?)?;
        if capture.process != process {
            return Err("PROCESS_IDENTITY_REJECTED".into());
        }
        Ok(capture)
    }
    pub fn new() -> Result<Self, String> {
        let mut candidates = Vec::new();
        if let Some(path) = std::env::var_os("CODEX_WINDOW_RESTORE_HELPER") {
            candidates.push(PathBuf::from(path));
        }
        if let Ok(executable) = std::env::current_exe() {
            if let Some(parent) = executable.parent() {
                candidates.push(parent.join("codex-window-restore"));
            }
        }
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".local/bin/codex-window-restore"));
        }
        candidates.push(PathBuf::from("/usr/local/bin/codex-window-restore"));
        candidates
            .into_iter()
            .find(|path| path.is_file())
            .map(|helper| Self { helper })
            .ok_or_else(|| "Codex window restore helper is not installed".into())
    }

    fn invoke(&self, args: &[String]) -> Result<serde_json::Value, String> {
        let output = Command::new(&self.helper)
            .args(args)
            .output()
            .map_err(|_| "Codex window restore helper could not start".to_string())?;
        if !output.status.success() {
            return Err(Self::helper_failure(&output.stderr));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_| "Codex window restore helper returned invalid data".into())
    }

    fn helper_failure(stderr: &[u8]) -> String {
        match stderr.strip_suffix(b"\n").unwrap_or(stderr) {
            b"WINDOW_NOT_FOUND" => "WINDOW_NOT_FOUND".into(),
            b"WINDOW_ACCESS_FAILED" => "WINDOW_ACCESS_FAILED".into(),
            b"WINDOW_GEOMETRY_FAILED" => "WINDOW_GEOMETRY_FAILED".into(),
            b"PROCESS_IDENTITY_REJECTED" => "PROCESS_IDENTITY_REJECTED".into(),
            _ => "Codex window restore helper rejected the request".into(),
        }
    }

    fn args_for_process(command: &str, process: ProcessIdentity) -> Vec<String> {
        vec![
            command.into(),
            "--expected-pid".into(),
            process.pid.to_string(),
            "--expected-birth".into(),
            process.birth_id,
        ]
    }

    fn parse_process(value: &serde_json::Value) -> Result<ProcessIdentity, String> {
        let pid = value["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or_else(|| "Window helper returned no process PID".to_string())?;
        let birth_id = value["birth_id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Window helper returned no process birth identity".to_string())?;
        ProcessIdentity::new(pid, birth_id)
    }

    fn parse_capture(value: serde_json::Value) -> Result<WindowCapture, String> {
        let process = Self::parse_process(&value["process"])?;
        let frame = parse_frame(&value["frame"])?;
        let screen = ScreenIdentity {
            display_id: value["screen"]["display_id"]
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .ok_or_else(|| "Window helper returned no display identity".to_string())?,
            frame: parse_frame(&value["screen"]["frame"])?,
        };
        let capture = WindowCapture {
            process,
            frame,
            screen,
        };
        if capture.is_valid() {
            Ok(capture)
        } else {
            Err("Window helper returned invalid window geometry".into())
        }
    }
}

impl WindowRestoreBackend for SystemWindowRestoreBackend {
    fn inspect_process(&mut self, expected_pid: u32) -> Result<ProcessIdentity, String> {
        let value = self.invoke(&[
            "inspect-process".to_string(),
            "--expected-pid".to_string(),
            expected_pid.to_string(),
        ])?;
        let identity = Self::parse_process(&value)?;
        if identity.is_for(expected_pid) {
            Ok(identity)
        } else {
            Err("Window helper returned a different process PID".into())
        }
    }

    fn capture_main_window(&mut self, process: ProcessIdentity) -> Result<WindowCapture, String> {
        Self::parse_capture(self.invoke(&Self::args_for_process("capture-window", process))?)
    }

    fn set_position(
        &mut self,
        process: ProcessIdentity,
        position: (f64, f64),
    ) -> Result<(), String> {
        let mut args = Self::args_for_process("set-position", process);
        args.extend([
            "--x".into(),
            position.0.to_string(),
            "--y".into(),
            position.1.to_string(),
        ]);
        self.invoke(&args).map(|_| ())
    }

    fn set_size(&mut self, process: ProcessIdentity, size: (f64, f64)) -> Result<(), String> {
        let mut args = Self::args_for_process("set-size", process);
        args.extend([
            "--width".into(),
            size.0.to_string(),
            "--height".into(),
            size.1.to_string(),
        ]);
        self.invoke(&args).map(|_| ())
    }

    fn read_main_window(&mut self, process: ProcessIdentity) -> Result<WindowCapture, String> {
        Self::parse_capture(self.invoke(&Self::args_for_process("read-window", process))?)
    }
}

fn parse_frame(value: &serde_json::Value) -> Result<WindowFrame, String> {
    let frame = WindowFrame {
        x: value["x"]
            .as_f64()
            .ok_or("Window helper returned invalid x")?,
        y: value["y"]
            .as_f64()
            .ok_or("Window helper returned invalid y")?,
        width: value["width"]
            .as_f64()
            .ok_or("Window helper returned invalid width")?,
        height: value["height"]
            .as_f64()
            .ok_or("Window helper returned invalid height")?,
    };
    frame
        .is_valid()
        .then_some(frame)
        .ok_or_else(|| "Window helper returned invalid frame".into())
}

#[cfg(test)]
#[path = "system_window_restore_backend.test.rs"]
mod tests;
