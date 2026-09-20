#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchTrigger {
    User,
    Auto,
    Shim,
}

impl SwitchTrigger {
    pub fn as_category(&self) -> &'static str {
        match self {
            SwitchTrigger::User => "USER_SWITCH",
            SwitchTrigger::Auto => "AUTO_SWITCH",
            SwitchTrigger::Shim => "SHIM_SWITCH",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SwitchTrigger::User => "user",
            SwitchTrigger::Auto => "auto",
            SwitchTrigger::Shim => "shim",
        }
    }
}

impl std::str::FromStr for SwitchTrigger {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(SwitchTrigger::Auto),
            "shim" => Ok(SwitchTrigger::Shim),
            _ => Ok(SwitchTrigger::User),
        }
    }
}
