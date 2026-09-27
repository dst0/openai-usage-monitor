/// What the helper verified for each planned window, in plan order: after
/// the relaunch, that the window is on its frame and copied its planned
/// task link; after recovery, that recovery did not leave it on one of its
/// own tasks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowTaskRestoreReport {
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
