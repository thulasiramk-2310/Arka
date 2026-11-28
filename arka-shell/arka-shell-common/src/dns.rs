#[derive(Debug, Clone, PartialEq)]
pub enum DnsStatus {
    Encrypted,
    Plaintext,
    Degraded,
    ForcedPlaintext,
    /// Encrypted DNS is switched off on the running connection only, so the
    /// network's own sign-in page can load. The saved profile is unchanged.
    PausedForSignIn,
    Error,
    Unknown(String),
}

impl DnsStatus {
    /// Only the daemon's explicit encrypted state confirms this control.
    pub fn is_encrypted(&self) -> bool {
        matches!(self, Self::Encrypted)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Encrypted       => "Encrypted",
            Self::Plaintext       => "Plaintext",
            Self::Degraded        => "Degraded",
            Self::ForcedPlaintext => "ForcedPlaintext",
            Self::PausedForSignIn => "PausedForSignIn",
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
            "PausedForSignIn" => Self::PausedForSignIn,
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

#[cfg(test)]
mod tests {
    use super::DnsStatus;

    #[test]
    fn only_encrypted_is_positive() {
        assert!(DnsStatus::from("Encrypted").is_encrypted());
        for value in ["Plaintext", "Degraded", "ForcedPlaintext", "PausedForSignIn", "Error", "Unknown", "Initializing", "", "DoT active", "future-state"] {
            assert!(!DnsStatus::from(value).is_encrypted(), "{value}");
        }
    }

    #[test]
    fn the_paused_status_round_trips_and_is_not_encrypted() {
        let s = DnsStatus::from("PausedForSignIn");
        assert_eq!(s, DnsStatus::PausedForSignIn);
        assert_eq!(s.as_str(), "PausedForSignIn");
        assert!(!s.is_encrypted());
    }
}
