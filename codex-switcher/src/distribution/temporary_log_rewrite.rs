use super::monitor_log_io_service::MonitorLogIoService;
use std::fs::File;

pub(super) struct TemporaryLogRewrite {
    parent: File,
    name: String,
    armed: bool,
}

impl TemporaryLogRewrite {
    pub(super) fn new(parent: File, name: String) -> Self {
        Self {
            parent,
            name,
            armed: true,
        }
    }

    pub(super) fn remove(&mut self) -> std::io::Result<()> {
        MonitorLogIoService::remove_child(&self.parent, &self.name, false)?;
        self.armed = false;
        Ok(())
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryLogRewrite {
    fn drop(&mut self) {
        if self.armed {
            let _ = MonitorLogIoService::remove_child(&self.parent, &self.name, false);
        }
    }
}
