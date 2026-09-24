#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCaptureMode {
    Captured,
    Absent,
    Skipped,
}
