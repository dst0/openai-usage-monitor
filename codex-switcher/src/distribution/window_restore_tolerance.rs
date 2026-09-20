#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestoreTolerance {
    pub position: f64,
    pub size: f64,
}

impl RestoreTolerance {
    pub fn new(position: f64, size: f64) -> Result<Self, String> {
        if !position.is_finite() || !size.is_finite() || position < 0.0 || size < 0.0 {
            return Err("Window restore tolerance must be finite and non-negative".into());
        }
        Ok(Self { position, size })
    }
}

impl Default for RestoreTolerance {
    fn default() -> Self {
        Self {
            position: 2.0,
            size: 2.0,
        }
    }
}
