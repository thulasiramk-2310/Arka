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
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
                warn!("config parse error: {e}, using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }
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
