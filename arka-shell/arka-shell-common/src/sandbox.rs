#[derive(Debug, Clone, PartialEq)]
pub enum SandboxStatus {
    Active,
    Inactive,
    Partial,
    Unknown(String),
}

impl SandboxStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Active    => "Active",
            Self::Inactive  => "Inactive",
            Self::Partial   => "Partial",
            Self::Unknown(s) => s,
        }
    }
}

impl From<String> for SandboxStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Active"   => Self::Active,
            "Inactive" => Self::Inactive,
            "Partial"  => Self::Partial,
            _          => Self::Unknown(s),
        }
    }
}

impl From<&str> for SandboxStatus {
    fn from(s: &str) -> Self { Self::from(s.to_owned()) }
}

impl Default for SandboxStatus {
    fn default() -> Self { Self::Unknown("Unknown".into()) }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrowserSandbox {
    Persistent,
    Ephemeral,
    PrivateWorkspace,
    None,
    Unknown(String),
}

impl BrowserSandbox {
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Persistent | Self::Ephemeral | Self::PrivateWorkspace)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Persistent       => "Persistent",
            Self::Ephemeral        => "Ephemeral",
            Self::PrivateWorkspace => "PrivateWorkspace",
            Self::None             => "None",
            Self::Unknown(s)       => s,
        }
    }
}

impl From<String> for BrowserSandbox {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Persistent"       => Self::Persistent,
            "Ephemeral"        => Self::Ephemeral,
            "PrivateWorkspace" => Self::PrivateWorkspace,
            "None"             => Self::None,
            _                  => Self::Unknown(s),
        }
    }
}

impl From<&str> for BrowserSandbox {
    fn from(s: &str) -> Self { Self::from(s.to_owned()) }
}

impl Default for BrowserSandbox {
    fn default() -> Self { Self::Unknown("Unknown".into()) }
}

#[cfg(test)]
mod tests {
    use super::{BrowserSandbox, SandboxStatus};

    #[test]
    fn failed_and_unknown_sandbox_states_are_not_active() {
        assert!(SandboxStatus::from("Active").is_active());
        for value in ["Inactive", "Partial", "Unknown", "Initializing", "", "future-state"] {
            assert!(!SandboxStatus::from(value).is_active(), "{value}");
        }
    }

    #[test]
    fn only_known_browser_modes_are_isolated() {
        for value in ["Persistent", "Ephemeral", "PrivateWorkspace"] {
            assert!(BrowserSandbox::from(value).is_isolated(), "{value}");
        }
        for value in ["None", "Unknown", "Initializing", "", "future-state"] {
            assert!(!BrowserSandbox::from(value).is_isolated(), "{value}");
        }
    }
}
