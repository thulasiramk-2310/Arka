#[derive(Debug, Clone, PartialEq)]
pub enum DnsStatus {
    Encrypted,
    Plaintext,
    Degraded,
    ForcedPlaintext,
    Error,
    Unknown(String),
}

impl DnsStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Encrypted       => "Encrypted",
            Self::Plaintext       => "Plaintext",
            Self::Degraded        => "Degraded",
            Self::ForcedPlaintext => "ForcedPlaintext",
            Self::Error           => "Error",
            Self::Unknown(s)      => s,
        }
    }
}

impl From<String> for DnsStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Encrypted"       => Self::Encrypted,
            "Plaintext"       => Self::Plaintext,
            "Degraded"        => Self::Degraded,
            "ForcedPlaintext" => Self::ForcedPlaintext,
            "Error"           => Self::Error,
            _                 => Self::Unknown(s),
        }
    }
}

impl From<&str> for DnsStatus {
    fn from(s: &str) -> Self { Self::from(s.to_owned()) }
}

impl Default for DnsStatus {
    fn default() -> Self { Self::Unknown("Unknown".into()) }
}
