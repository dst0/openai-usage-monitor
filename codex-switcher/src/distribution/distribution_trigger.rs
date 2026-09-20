use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionTrigger {
    Auto,
    User,
}

impl DistributionTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            DistributionTrigger::Auto => "auto",
            DistributionTrigger::User => "user",
        }
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, DistributionTrigger::Auto)
    }

    pub fn is_user(&self) -> bool {
        matches!(self, DistributionTrigger::User)
    }
}

impl fmt::Display for DistributionTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for DistributionTrigger {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "auto" | "automatic" | "daemon" => Ok(DistributionTrigger::Auto),
            _ => Ok(DistributionTrigger::User),
        }
    }
}
