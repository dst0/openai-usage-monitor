use super::app_lifecycle::AppLifecycle;
use super::window_capture_mode::WindowCaptureMode;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

pub struct MockAppLifecycle {
    pub running: AtomicBool,
    pub stop_calls: AtomicUsize,
    pub launch_calls: AtomicUsize,
    pub recovery_calls: AtomicUsize,
    pub capture_calls: AtomicUsize,
    pub restore_calls: AtomicUsize,
    pub rebind_calls: AtomicUsize,
    pub abort_calls: AtomicUsize,
    pub require_window_on_stability: Mutex<Option<bool>>,
    pub stop_error: Mutex<Option<String>>,
    pub corrupt_manifest_after_stop: Mutex<Option<PathBuf>>,
    pub stop_observer: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    pub launch_observer: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    pub recovery_error: Mutex<Option<String>>,
    pub recovery_observer: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    pub launch_error: Mutex<Option<String>>,
    pub capture_error: Mutex<Option<String>>,
    pub process_inspection_error: Mutex<Option<String>>,
    pub process_inspection_error_after: Mutex<Option<(usize, String)>>,
    pub process_inspection_calls: AtomicUsize,
    pub change_process_birth_after_first_inspection: AtomicBool,
    pub change_process_birth_after_launch: AtomicBool,
    pub capture_mode: Mutex<WindowCaptureMode>,
    pub restore_error: Mutex<Option<String>>,
    pub rebind_error: Mutex<Option<String>>,
    pub stability_error: Mutex<Option<String>>,
}

impl Default for MockAppLifecycle {
    fn default() -> Self {
        Self::new(true)
    }
}

impl MockAppLifecycle {
    pub fn new(running: bool) -> Self {
        Self {
            running: AtomicBool::new(running),
            stop_calls: AtomicUsize::new(0),
            launch_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            capture_calls: AtomicUsize::new(0),
            restore_calls: AtomicUsize::new(0),
            rebind_calls: AtomicUsize::new(0),
            abort_calls: AtomicUsize::new(0),
            require_window_on_stability: Mutex::new(None),
            stop_error: Mutex::new(None),
            corrupt_manifest_after_stop: Mutex::new(None),
            stop_observer: Mutex::new(None),
            launch_observer: Mutex::new(None),
            recovery_error: Mutex::new(None),
            recovery_observer: Mutex::new(None),
            launch_error: Mutex::new(None),
            capture_error: Mutex::new(None),
            process_inspection_error: Mutex::new(None),
            process_inspection_error_after: Mutex::new(None),
            process_inspection_calls: AtomicUsize::new(0),
            change_process_birth_after_first_inspection: AtomicBool::new(false),
            change_process_birth_after_launch: AtomicBool::new(false),
            capture_mode: Mutex::new(WindowCaptureMode::Captured),
            restore_error: Mutex::new(None),
            rebind_error: Mutex::new(None),
            stability_error: Mutex::new(None),
        }
    }

    pub fn set_recovery_error(&self, err: impl Into<String>) {
        *self.recovery_error.lock().unwrap() = Some(err.into());
    }

    pub fn observe_recovery(&self, observer: impl FnOnce() + Send + 'static) {
        *self.recovery_observer.lock().unwrap() = Some(Box::new(observer));
    }

    pub fn set_stop_error(&self, err: impl Into<String>) {
        *self.stop_error.lock().unwrap() = Some(err.into());
    }

    pub fn observe_stop(&self, observer: impl FnOnce() + Send + 'static) {
        *self.stop_observer.lock().unwrap() = Some(Box::new(observer));
    }

    pub fn observe_launch(&self, observer: impl FnOnce() + Send + 'static) {
        *self.launch_observer.lock().unwrap() = Some(Box::new(observer));
    }

    pub fn set_launch_error(&self, err: impl Into<String>) {
        *self.launch_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_capture_error(&self, err: impl Into<String>) {
        *self.capture_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_process_inspection_error(&self, err: impl Into<String>) {
        *self.process_inspection_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_process_inspection_error_after(&self, count: usize, err: impl Into<String>) {
        *self.process_inspection_error_after.lock().unwrap() = Some((count, err.into()));
    }

    pub fn change_process_birth_after_first_inspection(&self) {
        self.change_process_birth_after_first_inspection
            .store(true, Ordering::SeqCst);
    }

    pub fn change_process_birth_after_launch(&self) {
        self.change_process_birth_after_launch
            .store(true, Ordering::SeqCst);
    }

    pub fn set_capture_mode(&self, mode: WindowCaptureMode) {
        *self.capture_mode.lock().unwrap() = mode;
    }

    pub fn set_restore_error(&self, err: impl Into<String>) {
        *self.restore_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_rebind_error(&self, err: impl Into<String>) {
        *self.rebind_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_stability_error(&self, err: impl Into<String>) {
        *self.stability_error.lock().unwrap() = Some(err.into());
    }
}

impl AppLifecycle for MockAppLifecycle {
    fn is_app_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop_app(&self) -> Result<(), String> {
        self.stop_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.stop_error.lock().unwrap().clone() {
            return Err(error);
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(path) = self.corrupt_manifest_after_stop.lock().unwrap().as_ref() {
            std::fs::write(path, b"invalid recovery manifest")
                .map_err(|error| error.to_string())?;
        }
        if let Some(observer) = self.stop_observer.lock().unwrap().take() {
            observer();
        }
        Ok(())
    }

    fn launch_app(&self) -> Result<Vec<u32>, String> {
        self.launch_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(ref err) = *self.launch_error.lock().unwrap() {
            return Err(err.clone());
        }
        self.running.store(true, Ordering::SeqCst);
        if let Some(observer) = self.launch_observer.lock().unwrap().take() {
            observer();
        }
        self.process_inspection_calls.store(0, Ordering::SeqCst);
        if self
            .change_process_birth_after_launch
            .load(Ordering::SeqCst)
        {
            self.change_process_birth_after_first_inspection
                .store(true, Ordering::SeqCst);
        }
        Ok(vec![9999])
    }

    fn inspect_process(
        &self,
        pid: u32,
    ) -> Result<super::window_restore_process_identity::ProcessIdentity, String> {
        if !self.running.load(Ordering::SeqCst) || pid != 9999 {
            return Err("Desktop process is absent".into());
        }
        let inspected = self.process_inspection_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.process_inspection_error.lock().unwrap().clone() {
            return Err(error);
        }
        if let Some((after, error)) = self.process_inspection_error_after.lock().unwrap().clone() {
            if inspected >= after {
                return Err(error);
            }
        }
        let birth = if inspected > 0
            && self
                .change_process_birth_after_first_inspection
                .load(Ordering::SeqCst)
        {
            "other-birth"
        } else {
            "123:456789"
        };
        super::window_restore_process_identity::ProcessIdentity::new(pid, birth)
    }

    fn capture_window_bounds(
        &self,
        _operation_id: &str,
        _targets: &[String],
        _reason: &str,
        preserve_window_bounds: bool,
    ) -> Result<WindowCaptureMode, String> {
        if !preserve_window_bounds {
            self.inspect_process(9999)?;
            return Ok(WindowCaptureMode::Skipped);
        }
        self.capture_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.capture_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(*self.capture_mode.lock().unwrap())
    }

    fn restore_window_bounds(
        &self,
        _pid: u32,
        _operation_id: &str,
        _reason: &str,
    ) -> Result<(), String> {
        self.restore_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.restore_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(())
    }

    fn rebind_banner(&self, _pid: u32) -> Result<(), String> {
        self.rebind_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.rebind_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(())
    }

    fn abort_recovery(&self) {
        self.abort_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn recover_threads(&self, _targets: &[String]) -> Result<(), String> {
        self.recovery_calls.fetch_add(1, Ordering::SeqCst);
        let observer = self
            .recovery_observer
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(observer) = observer {
            observer();
        }
        if let Some(ref err) = *self.recovery_error.lock().unwrap() {
            return Err(err.clone());
        }
        Ok(())
    }

    fn verify_desktop_stable(&self, _pids: &[u32], require_window: bool) -> Result<(), String> {
        *self.require_window_on_stability.lock().unwrap() = Some(require_window);
        if let Some(ref err) = *self.stability_error.lock().unwrap() {
            return Err(err.clone());
        }
        Ok(())
    }

    fn notify_distribution_complete(&self) {}
}
