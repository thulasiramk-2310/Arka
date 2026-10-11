use crate::config::Config;
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

/// The ArkaOS zone (shipped in /usr/lib/firewalld/zones/arkaos.xml): rejects
/// unsolicited incoming connections; replies to the computer's own
/// connections are still allowed.
const ZONE: &str = "arkaos";

pub struct FirewallEnforcer;

impl FirewallEnforcer {
    pub fn new(_cfg: &Config) -> Self { Self }
}

#[async_trait::async_trait]
impl AsyncEnforcer for FirewallEnforcer {
    fn name(&self) -> &'static str { "firewall" }

    async fn enforce(&self) -> Result<(), ArkadError> {
        if crate::command::run("systemctl", &["is-active", "--quiet", "firewalld"]).await.is_err() {
            crate::command::run("systemctl", &["start", "firewalld"]).await?;
        }
        let out = crate::command::run("firewall-cmd", &["--get-default-zone"]).await?;
        if String::from_utf8_lossy(&out.stdout).trim() != ZONE {
            crate::command::run("firewall-cmd", &["--set-default-zone", ZONE]).await?;
        }
        // Interfaces the user moved to another zone are left alone; they show
        // up as "Needs attention" instead of being silently changed back.
        Ok(())
    }

    async fn update_state(&self, state: &SharedState) {
        // Check runtime rules, not just the zone's name: firewall-config and
        // other applications can add exceptions without changing that name.
        let (zones, rules, direct, passthroughs, policies, dhcp) = tokio::join!(
            crate::command::run("firewall-cmd", &["--get-active-zones"]),
            crate::command::run("firewall-cmd", &["--zone=arkaos", "--list-all"]),
            crate::command::run("firewall-cmd", &["--direct", "--get-all-rules"]),
            crate::command::run("firewall-cmd", &["--direct", "--get-all-passthroughs"]),
            crate::command::run("firewall-cmd", &["--list-all-policies"]),
            crate::command::run("firewall-cmd", &["--info-service=dhcpv6-client"]),
        );
        let on = match (zones, rules, direct, passthroughs, policies, dhcp) {
            (Ok(z), Ok(r), Ok(d), Ok(t), Ok(p), Ok(h)) =>
                only_zone_active(&String::from_utf8_lossy(&z.stdout), ZONE)
                && restrictive_zone(&String::from_utf8_lossy(&r.stdout))
                && String::from_utf8_lossy(&d.stdout).trim().is_empty()
                && String::from_utf8_lossy(&t.stdout).trim().is_empty()
                && !has_inbound_policy(&String::from_utf8_lossy(&p.stdout))
                && dhcp_service_is_narrow(&String::from_utf8_lossy(&h.stdout)),
            _ => false,
        };
        state.write().await.firewall = on;
    }
}

/// `firewall-cmd --get-active-zones` lists each zone name on an unindented
/// line, followed by indented "interfaces:"/"sources:" lines. Configured only
/// when at least one zone is active and every active zone is ours.
fn only_zone_active(text: &str, zone: &str) -> bool {
    let zones: Vec<&str> = text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with(char::is_whitespace))
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .collect();
    !zones.is_empty() && zones.iter().all(|z| *z == zone)
}

// Missing fields or multiline rules are unconfirmed, never a successful check.
fn restrictive_zone(text: &str) -> bool {
    let field = |name: &str| text.lines().find_map(|line| {
        let (key, value) = line.trim().split_once(':')?;
        (key == name).then_some(value.trim())
    });
    field("target") == Some("%%REJECT%%")
        && field("services").is_some_and(|v| v.split_whitespace().all(|s| s == "dhcpv6-client"))
        && ["ports", "protocols", "source-ports", "forward-ports", "rich rules"]
            .iter().all(|key| field(key) == Some(""))
        && !text.lines().any(|l| l.trim_start().starts_with("rule "))
        && !text.lines().any(|l| l.trim_start().starts_with("port="))
}

// Service definitions can be overridden without changing the service name.
fn dhcp_service_is_narrow(text: &str) -> bool {
    let field = |name: &str| text.lines().find_map(|line| {
        let (key, value) = line.trim().split_once(':')?;
        (key == name).then_some(value.trim())
    });
    field("ports") == Some("546/udp")
        && field("destination") == Some("ipv6:fe80::/64")
        && ["protocols", "source-ports"].iter().all(|key| field(key) == Some(""))
        && ["helpers", "modules", "includes"].iter().all(|key| field(key).unwrap_or("").is_empty())
}

// An independent policy can let traffic in around the zone. Disabled policies
// do nothing. An active policy into this computer (egress HOST or ANY) is
// acceptable only if it opens no ports, services or protocols and its rules
// only accept ICMP message types: that is Fedora's allow-host-ipv6, which IPv6
// needs (neighbour discovery, router advertisements). Anything else, or
// anything we can't parse, is unconfirmed.
fn has_inbound_policy(text: &str) -> bool {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        if !line.starts_with(char::is_whitespace) { blocks.push(vec![line]); }
        else if let Some(b) = blocks.last_mut() { b.push(line); }
        else { return true; }
    }
    blocks.iter().any(|b| !policy_is_harmless(b))
}

fn policy_is_harmless(block: &[&str]) -> bool {
    if block[0].contains("(disabled)") { return true; }
    let field = |name: &str| block[1..].iter().find_map(|l| {
        let (k, v) = l.trim().split_once(':')?;
        (k == name).then_some(v.trim())
    });
    let into_host = field("egress-zones").is_none_or(|v| v.split_whitespace().any(|z| z == "HOST" || z == "ANY"));
    let from_host = field("ingress-zones") == Some("HOST");
    if from_host || !into_host { return true; }
    let opens_nothing = ["services", "ports", "protocols", "source-ports", "forward-ports"]
        .iter().all(|k| field(k) == Some(""))
        && field("target").is_some_and(|t| t != "ACCEPT");
    // Judge rich rules by their keywords (whitespace-separated), never by
    // substrings: ICMP type names such as "mld-listener-report" contain "port".
    let only_icmp = block[1..].iter().filter(|l| l.trim_start().starts_with("rule ")).all(|l| {
        let words: Vec<&str> = l.split_whitespace().collect();
        words.last() == Some(&"accept")
            && words.iter().any(|w| *w == "icmp-type")
            && words.iter().all(|w| matches!(w.split('=').next(), Some("rule" | "family" | "icmp-type" | "name" | "accept")))
    });
    opens_nothing && only_icmp
}

#[cfg(test)]
mod tests {
    use super::*;
    const CLOSED: &str = "arkaos (active)\n  target: %%REJECT%%\n  services: dhcpv6-client\n  ports: \n  protocols: \n  source-ports: \n  forward-ports: \n  rich rules: \n";
    #[test]
    fn service_overrides_cannot_open_extra_ports() {
        let service = "dhcpv6-client\n  ports: 546/udp\n  protocols: \n  source-ports: \n  helpers: \n  destination: ipv6:fe80::/64\n  includes: \n";
        assert!(dhcp_service_is_narrow(service));
        assert!(!dhcp_service_is_narrow(&service.replace("546/udp", "546/udp 22/tcp")));
        assert!(!dhcp_service_is_narrow(&service.replace("includes: ", "includes: ssh")));
        assert!(!dhcp_service_is_narrow(""));
    }
    #[test]
    fn zone_name_does_not_prove_restrictive_rules() {
        assert!(restrictive_zone(CLOSED));
        for (old, new) in [("%%REJECT%%", "ACCEPT"), ("services: dhcpv6-client", "services: dhcpv6-client ssh"), ("ports: ", "ports: 8080/tcp"), ("protocols: ", "protocols: tcp"), ("source-ports: ", "source-ports: 22/tcp")] {
            assert!(!restrictive_zone(&CLOSED.replace(old, new)));
        }
        assert!(!restrictive_zone(&format!("{CLOSED}    rule family=ipv4 accept\n")));
        assert!(!restrictive_zone("arkaos\n"));
    }
    // As printed by `firewall-cmd --list-all-policies` on Fedora 44 (VM, 2026-10-05).
    const FEDORA_POLICIES: &str = "allow-host-ipv6 (active)\n  priority: -15000\n  target: CONTINUE\n  ingress-zones: ANY\n  egress-zones: HOST\n  services: \n  ports: \n  protocols: \n  masquerade: no\n  forward-ports: \n  source-ports: \n  icmp-blocks: \n  rich rules: \n\trule family=\"ipv6\" icmp-type name=\"neighbour-advertisement\" accept\n\trule family=\"ipv6\" icmp-type name=\"neighbour-solicitation\" accept\n\trule family=\"ipv6\" icmp-type name=\"router-advertisement\" accept\n\trule family=\"ipv6\" icmp-type name=\"redirect\" accept\n\
gateway-world-to-HOST (disabled)\n  target: CONTINUE\n  ingress-zones: external public\n  egress-zones: HOST\n  services: ssh\n  ports: \n  protocols: \n  forward-ports: \n  source-ports: \n  rich rules: \n";

    // Captured verbatim from `firewall-cmd --list-all-policies` in an ArkaOS VM
    // (Fedora 44, 2026-10-11). Includes mld-listener-report, whose name contains
    // "port" and once made the check fail.
    const REAL_F44: &str = include_str!("testdata/fedora44-list-all-policies.txt");

    #[test]
    fn the_real_fedora_44_policy_listing_is_harmless() {
        assert!(REAL_F44.contains("mld-listener-report"));
        assert!(!has_inbound_policy(REAL_F44));
    }

    #[test]
    fn rich_rules_are_judged_by_keyword_not_substring() {
        let rule = |r: &str| REAL_F44.replacen("\trule family=\"ipv6\" icmp-type name=\"redirect\" accept", r, 1);
        for opened in [
            "\trule family=\"ipv4\" port port=\"22\" protocol=\"tcp\" accept",
            "\trule family=\"ipv4\" service name=\"ssh\" accept",
            "\trule family=\"ipv4\" source address=\"10.0.0.0/8\" accept",
            "\trule family=\"ipv6\" icmp-type name=\"echo-request\" source address=\"::/0\" accept",
            "\trule family=\"ipv4\" forward-port port=\"80\" protocol=\"tcp\" to-port=\"8080\" accept",
            "\trule family=\"ipv4\" accept",
        ] {
            assert!(has_inbound_policy(&rule(opened)), "{opened}");
        }
    }

    #[test]
    fn fedoras_ipv6_control_policy_and_disabled_policies_are_harmless() {
        assert!(!has_inbound_policy(FEDORA_POLICIES));
        assert!(!has_inbound_policy(""));
        assert!(!has_inbound_policy("outbound (active)\n  ingress-zones: HOST\n  egress-zones: ANY\n  services: ssh\n"));
    }

    #[test]
    fn an_active_policy_that_opens_anything_inbound_is_unconfirmed() {
        let opened = FEDORA_POLICIES.replace("allow-host-ipv6 (active)\n  priority: -15000\n  target: CONTINUE\n  ingress-zones: ANY\n  egress-zones: HOST\n  services: ",
                                               "allow-host-ipv6 (active)\n  priority: -15000\n  target: CONTINUE\n  ingress-zones: ANY\n  egress-zones: HOST\n  services: ssh");
        assert!(has_inbound_policy(&opened));
        for (old, new) in [("target: CONTINUE", "target: ACCEPT"), ("  ports: \n", "  ports: 22/tcp\n"),
                           ("name=\"redirect\" accept", "name=\"redirect\" accept\n\trule family=\"ipv4\" port port=\"22\" protocol=\"tcp\" accept"),
                           ("gateway-world-to-HOST (disabled)", "gateway-world-to-HOST (active)")] {
            assert!(has_inbound_policy(&FEDORA_POLICIES.replacen(old, new, 1)), "{new}");
        }
        assert!(has_inbound_policy("custom\n"), "unknown shape is unconfirmed");
        assert!(has_inbound_policy("  orphan: line\n"));
    }
    #[test]
    fn every_active_zone_must_be_the_arkaos_zone() {
        assert!(only_zone_active("arkaos (default)\n  interfaces: wlp2s0\n", "arkaos"));
        assert!(!only_zone_active("arkaos\n  interfaces: wlp2s0\nhome\n  interfaces: enp3s0\n", "arkaos"));
        assert!(!only_zone_active("FedoraWorkstation (default)\n  interfaces: wlp2s0\n", "arkaos"));
        assert!(!only_zone_active("", "arkaos"));
    }
}
