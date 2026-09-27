/// What the helper verified after placing each planned window: every
/// window's WindowServer ID in plan order, and whether that window copied
/// its planned task link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowTaskRestoreReport {
    pub window_ids: Vec<u32>,
    pub verified: Vec<bool>,
    pub clipboard_restored: bool,
}

impl WindowTaskRestoreReport {
    pub fn verified_count(&self) -> usize {
        self.verified.iter().filter(|verified| **verified).count()
    }

    pub fn is_complete(&self) -> bool {
        self.verified.iter().all(|verified| *verified)
    }
}
