#[derive(Debug, Clone, PartialEq)]
pub enum SandboxStatus {
    Active,
    Inactive,
    Partial,
    Unknown(String),
}

impl SandboxStatus {
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

#[derive(Debug, Clone, PartialEq)]
pub enum BrowserSandbox {
    Persistent,
    Ephemeral,
    PrivateWorkspace,
    None,
    Unknown(String),
}

impl BrowserSandbox {
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
