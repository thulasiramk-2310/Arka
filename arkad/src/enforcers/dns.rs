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
        let expected = self.conf_content();
        let changed = std::fs::read_to_string(RESOLVED_CONF).map(|s| s != expected).unwrap_or(true);
        std::fs::create_dir_all("/etc/systemd/resolved.conf.d")?;
        if changed { std::fs::write(RESOLVED_CONF, expected)?; }
        let running = crate::command::run("systemctl", &["is-active", "--quiet", "systemd-resolved"]).await.is_ok();
        if changed || !running {
            crate::command::run("systemctl", &["restart", "systemd-resolved"]).await?;
        }
        // Conflicting per-link/VPN settings stay unconfirmed. Repeated retries
        // must not restart a healthy resolver or silently rewrite a user's VPN.
        Ok(())
    }

    async fn update_state(&self, state: &SharedState) {
        let configured = std::fs::read_to_string(RESOLVED_CONF)
            .map(|c| c == self.conf_content()).unwrap_or(false);
        let stub = std::fs::read_to_string("/etc/resolv.conf")
            .map(|s| uses_stub_only(&s)).unwrap_or(false);
        let status = match crate::command::run("resolvectl", &["--no-pager", "status"]).await {
            Ok(out) if configured && stub && strict_tls_scopes(&String::from_utf8_lossy(&out.stdout)) => DnsStatus::Encrypted,
            Ok(_) => DnsStatus::Degraded,
            Err(_) => DnsStatus::Error,
        };
        state.write().await.dns_status = status;
    }
}

fn uses_stub_only(contents: &str) -> bool {
    let servers: Vec<_> = contents.lines().filter_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("nameserver")).then(|| words.next().unwrap_or(""))
    }).collect();
    !servers.is_empty() && servers.iter().all(|s| *s == "127.0.0.53")
}

// Check every scope that has DNS servers, including VPN/split-DNS links.
// A recognized strict TLS token is required; opportunistic TLS is insufficient.
// This observes resolver policy, not arbitrary application traffic or uptime.
fn strict_tls_scopes(text: &str) -> bool {
    let mut scopes = Vec::new();
    let mut current: Option<(bool, bool)> = None; // strict TLS, has DNS servers
    for line in text.lines() {
        let line = line.trim();
        if line == "Global" || line.starts_with("Link ") {
            if let Some(scope) = current.take() { scopes.push(scope); }
            current = Some((false, false));
        } else if let Some(scope) = current.as_mut() {
            if let Some(protocols) = line.strip_prefix("Protocols:") {
                scope.0 = protocols.split_whitespace().any(|v| v == "+DNSOverTLS");
            }
            if let Some(servers) = line.strip_prefix("DNS Servers:") {
                scope.1 = !servers.trim().is_empty();
            }
        }
    }
    if let Some(scope) = current { scopes.push(scope); }
    let dns_scopes: Vec<_> = scopes.into_iter().filter(|s| s.1).collect();
    !dns_scopes.is_empty() && dns_scopes.iter().all(|s| s.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_resolver_bypass_and_empty_configuration() {
        assert!(uses_stub_only("# local resolver\nnameserver 127.0.0.53\n"));
        assert!(!uses_stub_only("nameserver 127.0.0.53\nnameserver 8.8.8.8"));
        assert!(!uses_stub_only("# nameserver 127.0.0.53"));
    }
    #[test]
    fn plaintext_split_dns_cannot_hide_behind_encrypted_global_dns() {
        let global = "Global\n Protocols: -LLMNR +DNSOverTLS\n DNS Servers: 9.9.9.9#dns.quad9.net\n";
        assert!(strict_tls_scopes(global));
        assert!(!strict_tls_scopes(&format!("{global}Link 2 (vpn)\n Protocols: -DNSOverTLS\n DNS Servers: 10.0.0.1")));
        assert!(!strict_tls_scopes(&global.replace("+DNSOverTLS", "DNSOverTLS=opportunistic")));
        assert!(!strict_tls_scopes(""));
        assert!(!strict_tls_scopes("Global\n Protocols: +DNSOverTLS"));
    }
}
