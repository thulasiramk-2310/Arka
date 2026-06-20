use arka_shell_common::DnsStatus;
use crate::config::Config;
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

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

#[async_trait::async_trait]
impl AsyncEnforcer for DnsEnforcer {
    fn name(&self) -> &'static str { "dns" }

    async fn enforce(&self) -> Result<(), ArkadError> {
        let content = self.conf_content();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all("/etc/systemd/resolved.conf.d")
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            std::fs::write(RESOLVED_CONF, &content)
                .map_err(|e| ArkadError::Enforce(e.to_string()))?;
            let _ = std::process::Command::new("systemctl")
                .args(["try-reload-or-restart", "systemd-resolved"])
                .status();
            Ok::<(), ArkadError>(())
        }).await?
    }

    async fn update_state(&self, state: &SharedState) {
        let expected = self.conf_content();
        let status = tokio::task::spawn_blocking(move || {
            std::fs::read_to_string(RESOLVED_CONF)
                .map(|c| if c == expected { DnsStatus::Encrypted } else { DnsStatus::Plaintext })
                .unwrap_or(DnsStatus::Error)
        }).await.unwrap_or(DnsStatus::Error);
        state.write().await.dns_status = status;
    }
}
