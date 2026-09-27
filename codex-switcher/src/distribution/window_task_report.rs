/// The only result an explicit window-task diagnostic returns: counts and
/// whether the user's clipboard was put back. Task IDs never reach Rust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowTaskReport {
    pub windows: usize,
    pub verified: usize,
    pub clipboard_restored: bool,
}
