use serde::Deserialize;
use std::fs;
use tracing::warn;

const CONFIG_PATH: &str = "/etc/arkad/arkad.toml";

#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,
    #[serde(default)]
    pub dns: DnsConfig,
    #[serde(default)]
    pub hostname: HostnameConfig,
}

#[derive(Deserialize)]
pub struct DnsConfig {
    #[serde(default = "default_dns_server")]
    pub server: String,
    #[serde(default = "default_dns_name")]
    pub tls_name: String,
}

#[derive(Deserialize)]
pub struct HostnameConfig {
    #[serde(default = "default_hostname")]
    pub name: String,
}

impl Config {
    pub fn load() -> Self {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => Self::parse(&contents),
            Err(_) => Self::default(),
        }
    }

    /// Read the settings from text. Every value ends up in a system file or on a command
    /// line, so one that is not what it claims to be is replaced by the default, with a
    /// warning, rather than passed on.
    pub fn parse(contents: &str) -> Self {
        let mut cfg: Self = toml::from_str(contents).unwrap_or_else(|e| {
            warn!("config parse error: {e}, using defaults");
            Self::default()
        });
        if cfg.dns.server.parse::<std::net::IpAddr>().is_err() {
            warn!("dns.server is not an IP address, using the default");
            cfg.dns.server = default_dns_server();
        }
        if !is_dns_name(&cfg.dns.tls_name) {
            warn!("dns.tls_name is not a host name, using the default");
            cfg.dns.tls_name = default_dns_name();
        }
        if !is_label(&cfg.hostname.name) || cfg.hostname.name.chars().any(|c| c.is_ascii_uppercase()) {
            warn!("hostname.name is not a lower-case host name, using the default");
            cfg.hostname.name = default_hostname();
        }
        cfg
    }
}

/// One part of a host name: letters, digits and hyphens, not starting or ending with a hyphen.
fn is_label(label: &str) -> bool {
    (1..=63).contains(&label.len())
        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !label.starts_with('-')
        && !label.ends_with('-')
}

/// A full host name such as `dns.quad9.net`.
fn is_dns_name(name: &str) -> bool {
    name.len() <= 253 && name.contains('.') && name.split('.').all(is_label)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            check_interval_secs: default_check_interval(),
            dns: DnsConfig::default(),
            hostname: HostnameConfig::default(),
        }
    }
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            server: default_dns_server(),
            tls_name: default_dns_name(),
        }
    }
}

impl Default for HostnameConfig {
    fn default() -> Self {
        Self {
            name: default_hostname(),
        }
    }
}

fn default_check_interval() -> u64 { 60 }
fn default_dns_server() -> String { "9.9.9.9".into() }
fn default_dns_name() -> String { "dns.quad9.net".into() }
fn default_hostname() -> String { "arka".into() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_values_are_kept() {
        let cfg = Config::parse("check_interval_secs = 30\n[dns]\nserver = \"2620:fe::fe\"\ntls_name = \"dns.quad9.net\"\n[hostname]\nname = \"my-laptop\"\n");
        assert_eq!((cfg.check_interval_secs, cfg.dns.server.as_str(), cfg.dns.tls_name.as_str(), cfg.hostname.name.as_str()), (30, "2620:fe::fe", "dns.quad9.net", "my-laptop"));
    }

    #[test]
    fn values_that_could_add_lines_or_options_are_replaced() {
        // a new line would add a setting to the resolver file; a leading hyphen is read as an option
        let cfg = Config::parse("[dns]\nserver = \"9.9.9.9\\nDNSOverTLS=no\"\ntls_name = \"x\\nFallbackDNS=1.1.1.1\"\n[hostname]\nname = \"--transient\"\n");
        assert_eq!((cfg.dns.server.as_str(), cfg.dns.tls_name.as_str(), cfg.hostname.name.as_str()), ("9.9.9.9", "dns.quad9.net", "arka"));
        for name in ["", "Arka", "a b", "a.b", "-a", "a-", &"a".repeat(64)] {
            assert_eq!(Config::parse(&format!("[hostname]\nname = \"{name}\"\n")).hostname.name, "arka", "{name:?}");
        }
        for name in ["quad9", "dns..net", "dns.quad9.net#x", "dns.quad9.net "] {
            assert_eq!(Config::parse(&format!("[dns]\ntls_name = \"{name}\"\n")).dns.tls_name, "dns.quad9.net", "{name:?}");
        }
    }

    #[test]
    fn unreadable_text_gives_the_defaults() {
        let cfg = Config::parse("this is not toml [");
        assert_eq!((cfg.check_interval_secs, cfg.dns.server.as_str(), cfg.hostname.name.as_str()), (60, "9.9.9.9", "arka"));
    }
}
