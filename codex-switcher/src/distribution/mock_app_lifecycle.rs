use super::app_lifecycle::AppLifecycle;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

pub struct MockAppLifecycle {
    pub running: AtomicBool,
    pub stop_calls: AtomicUsize,
    pub launch_calls: AtomicUsize,
    pub recovery_calls: AtomicUsize,
    pub capture_calls: AtomicUsize,
    pub restore_calls: AtomicUsize,
    pub stop_error: Mutex<Option<String>>,
    pub recovery_error: Mutex<Option<String>>,
    pub launch_error: Mutex<Option<String>>,
    pub capture_error: Mutex<Option<String>>,
    pub restore_error: Mutex<Option<String>>,
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
            stop_error: Mutex::new(None),
            recovery_error: Mutex::new(None),
            launch_error: Mutex::new(None),
            capture_error: Mutex::new(None),
            restore_error: Mutex::new(None),
            stability_error: Mutex::new(None),
        }
    }

    pub fn set_recovery_error(&self, err: impl Into<String>) {
        *self.recovery_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_stop_error(&self, err: impl Into<String>) {
        *self.stop_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_launch_error(&self, err: impl Into<String>) {
        *self.launch_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_capture_error(&self, err: impl Into<String>) {
        *self.capture_error.lock().unwrap() = Some(err.into());
    }

    pub fn set_restore_error(&self, err: impl Into<String>) {
        *self.restore_error.lock().unwrap() = Some(err.into());
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
        Ok(())
    }

    fn launch_app(&self) -> Result<Vec<u32>, String> {
        self.launch_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(ref err) = *self.launch_error.lock().unwrap() {
            return Err(err.clone());
        }
        self.running.store(true, Ordering::SeqCst);
        Ok(vec![9999])
    }

    fn capture_window_bounds(
        &self,
        _operation_id: &str,
        _targets: &[String],
        _reason: &str,
    ) -> Result<(), String> {
        self.capture_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.capture_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(())
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

    fn recover_threads(&self, _targets: &[String]) -> Result<(), String> {
        self.recovery_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(ref err) = *self.recovery_error.lock().unwrap() {
            return Err(err.clone());
        }
        Ok(())
    }

    fn verify_desktop_stable(&self, _pids: &[u32]) -> Result<(), String> {
        if let Some(ref err) = *self.stability_error.lock().unwrap() {
            return Err(err.clone());
        }
        Ok(())
    }

    fn notify_distribution_complete(&self) {}
}
