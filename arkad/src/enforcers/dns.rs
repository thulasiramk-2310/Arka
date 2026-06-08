use crate::config::Config;
use super::Enforcer;

const RESOLVED_CONF: &str = "/etc/systemd/resolved.conf.d/99-arkad-dot.conf";

pub struct DnsEnforcer {
    server: String,
    tls_name: String,
}

impl DnsEnforcer {
    pub fn new(cfg: &Config) -> Self {
        Self {
            server: cfg.dns.server.clone(),
            tls_name: cfg.dns.tls_name.clone(),
        }
    }

    fn conf_content(&self) -> String {
        format!(
            "[Resolve]\nDNS={}#{}\nDNSOverTLS=yes\nFallbackDNS=\nDNSSEC=allow-downgrade\n",
            self.server, self.tls_name
        )
    }
}

impl Enforcer for DnsEnforcer {
    fn name(&self) -> &'static str { "dns" }

    fn enforce(&self) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all("/etc/systemd/resolved.conf.d")?;
        std::fs::write(RESOLVED_CONF, self.conf_content())?;
        let _ = std::process::Command::new("systemctl")
            .args(["try-reload-or-restart", "systemd-resolved"])
            .status();
        Ok(())
    }

    fn verify(&self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(std::fs::read_to_string(RESOLVED_CONF)
            .map(|c| c == self.conf_content())
            .unwrap_or(false))
    }
}
