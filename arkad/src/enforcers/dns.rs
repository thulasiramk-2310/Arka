use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arka_shell_common::DnsStatus;
use crate::config::Config;
use crate::error::ArkadError;
use crate::state::SharedState;
use super::AsyncEnforcer;

const RESOLVED_CONF: &str = "/etc/systemd/resolved.conf.d/99-arkad-dot.conf";

type EventLog = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

pub struct DnsEnforcer {
    server: String,
    tls_name: String,
    log: EventLog,
    pauses: Mutex<PauseTracker>,
    probe: Arc<dyn NmProbe>,
}

/// Remembers which connections have encrypted DNS paused for a sign-in page
/// (the running connection's connection.dns-over-tls is "no" while the saved
/// profile is not), so the start and the end are each written to the event log
/// once. It also counts consecutive checks in which the network reports full
/// internet access on the paused device itself: after two, the pause is lifted
/// by the daemon (the backstop for a tray that was closed or never saw the
/// sign-in finish). Each connection is counted on its own.
#[derive(Default)]
struct PauseTracker {
    active: BTreeSet<String>,
    full: BTreeMap<String, (u8, Instant)>,
}

/// Two "full access" checks closer together than this count as one: a reconcile
/// can ask for the status twice within a moment.
const FULL_CHECK_GAP: Duration = Duration::from_secs(20);

impl PauseTracker {
    /// `paused` is `Some` only when this check could tell which connections are
    /// paused (none included). `None` changes nothing, so a failed check never
    /// logs an end.
    fn observe(&mut self, paused: Option<BTreeSet<String>>) -> Vec<(&'static str, String)> {
        let Some(now) = paused else { return Vec::new() };
        let mut events = Vec::new();
        for name in now.difference(&self.active) {
            events.push(("paused", format!(
                "Encrypted DNS is paused on the connection \"{}\" for its sign-in page: the running connection has connection.dns-over-tls \"no\"; the saved profile is unchanged",
                shown(name)
            )));
        }
        for name in self.active.difference(&now) {
            events.push(("paused-ended", format!(
                "Encrypted DNS is no longer paused on the connection \"{}\"",
                shown(name)
            )));
        }
        self.active = now;
        let active = &self.active;
        self.full.retain(|name, _| active.contains(name));
        events
    }

    /// Feed one reading for a paused connection's device: `full` is whether it
    /// has full access. True when that has held on two checks in a row.
    fn note_full(&mut self, conn: &str, full: bool, at: Instant) -> bool {
        if !full {
            self.full.remove(conn);
            return false;
        }
        let entry = self.full.entry(conn.to_string()).or_insert((0, at));
        // The first check counts at once; a later one counts only after the gap.
        if entry.0 > 0 && at.duration_since(entry.1) < FULL_CHECK_GAP {
            return false;
        }
        *entry = (entry.0 + 1, at);
        if entry.0 >= 2 {
            self.full.remove(conn);
            return true;
        }
        false
    }
}

/// NetworkManager's NMConnectivityState FULL.
const NM_CONNECTIVITY_FULL: u32 = 4;

/// What arkad asks NetworkManager over D-Bus. A trait so tests can answer
/// without a bus.
#[async_trait::async_trait]
trait NmProbe: Send + Sync {
    /// The running connection's connection.dns-over-tls (Device.GetAppliedConnection):
    /// Some(-1) default, Some(0) no, None when it is unset or cannot be read.
    async fn applied_dns_over_tls(&self, iface: &str) -> Option<i32>;
    /// The device's own (Ip4Connectivity, Ip6Connectivity). Not the Manager's
    /// overall value: a working ethernet link must not lift the pause on Wi-Fi
    /// that is still waiting for its sign-in.
    async fn device_connectivity(&self, iface: &str) -> Option<(u32, u32)>;
}

struct DbusProbe;

const NM_DEST: &str = "org.freedesktop.NetworkManager";

#[async_trait::async_trait]
impl NmProbe for DbusProbe {
    async fn applied_dns_over_tls(&self, iface: &str) -> Option<i32> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let conn = zbus::Connection::system().await.ok()?;
            let mgr = zbus::Proxy::new(&conn, NM_DEST, "/org/freedesktop/NetworkManager", NM_DEST).await.ok()?;
            let path: zbus::zvariant::OwnedObjectPath = mgr.call("GetDeviceByIpIface", &(iface,)).await.ok()?;
            let dev = zbus::Proxy::new(&conn, NM_DEST, path.as_str(), "org.freedesktop.NetworkManager.Device").await.ok()?;
            let (settings, _version): (
                std::collections::HashMap<String, std::collections::HashMap<String, zbus::zvariant::OwnedValue>>,
                u64,
            ) = dev.call("GetAppliedConnection", &(0u32,)).await.ok()?;
            let value = settings.get("connection")?.get("dns-over-tls")?.try_clone().ok()?;
            i32::try_from(value).ok()
        }).await.ok().flatten()
    }

    async fn device_connectivity(&self, iface: &str) -> Option<(u32, u32)> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let conn = zbus::Connection::system().await.ok()?;
            let mgr = zbus::Proxy::new(&conn, NM_DEST, "/org/freedesktop/NetworkManager", NM_DEST).await.ok()?;
            let path: zbus::zvariant::OwnedObjectPath = mgr.call("GetDeviceByIpIface", &(iface,)).await.ok()?;
            let dev = zbus::Proxy::new(&conn, NM_DEST, path.as_str(), "org.freedesktop.NetworkManager.Device").await.ok()?;
            let ip4 = dev.get_property::<u32>("Ip4Connectivity").await.ok()?;
            let ip6 = dev.get_property::<u32>("Ip6Connectivity").await.ok()?;
            Some((ip4, ip6))
        }).await.ok().flatten()
    }
}

/// A connection name as written to the event log. The name is chosen outside
/// arkad, so control characters are replaced and the length is bounded.
fn shown(name: &str) -> String {
    let mut out: String = name.chars().take(64).map(|c| if c.is_control() { '?' } else { c }).collect();
    if name.chars().count() > 64 { out.push_str("..."); }
    out
}

impl DnsEnforcer {
    pub fn new(cfg: &Config) -> Self {
        Self::with_log(cfg, Arc::new(crate::log::log_event))
    }

    fn with_log(cfg: &Config, log: EventLog) -> Self {
        Self::with_log_and_probe(cfg, log, Arc::new(DbusProbe))
    }

    fn with_log_and_probe(cfg: &Config, log: EventLog, probe: Arc<dyn NmProbe>) -> Self {
        Self {
            server: cfg.dns.server.clone(),
            tls_name: cfg.dns.tls_name.clone(),
            log,
            pauses: Mutex::new(PauseTracker::default()),
            probe,
        }
    }

    /// Logs pauses that started or ended since the last check.
    fn note_pauses(&self, paused: Option<BTreeSet<String>>) {
        let events = self.pauses.lock().unwrap_or_else(|p| p.into_inner()).observe(paused);
        for (ev, msg) in events {
            (self.log)("dns", ev, &msg);
        }
    }

    /// While a pause is active: after two checks in a row in which the paused
    /// device itself has full internet access (IPv4 or IPv6), turn encrypted DNS
    /// back on for that connection.
    async fn lift_pause_when_online(&self, nmcli: &str, paused: &BTreeMap<String, String>) {
        for (conn, dev) in paused {
            let full = self.probe.device_connectivity(dev).await
                .is_some_and(|(ip4, ip6)| ip4 == NM_CONNECTIVITY_FULL || ip6 == NM_CONNECTIVITY_FULL);
            let lift = self.pauses.lock().unwrap_or_else(|p| p.into_inner()).note_full(conn, full, Instant::now());
            if !lift { continue; }
            let msg = if crate::command::run(nmcli, &["device", "reapply", dev]).await.is_ok() {
                format!("The network reports full internet access, so encrypted DNS was turned back on for the connection \"{}\"", shown(conn))
            } else {
                format!("The network reports full internet access, but turning encrypted DNS back on for the connection \"{}\" failed", shown(conn))
            };
            (self.log)("dns", "paused-restored", &msg);
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
        // `excepted` stays None unless this check can tell which connections
        // use the exception.
        let mut excepted = None;
        let mut paused: Option<BTreeSet<String>> = None;
        let mut paused_links = BTreeMap::new();
        let status = match crate::command::run("resolvectl", &["--no-pager", "status"]).await {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                if configured && stub && strict_tls_scopes(&text) {
                    excepted = Some(BTreeSet::new());
                    DnsStatus::Encrypted
                } else if !(configured && stub) {
                    DnsStatus::Degraded
                } else if let Some(links) = classify_plaintext_links("nmcli", self.probe.as_ref(), &text).await {
                    if links.paused.is_empty() {
                        // The user chose "allow unencrypted DNS on this network only"
                        // (NetworkManager connection.dns-over-tls=no). Report it as
                        // such, never as encrypted.
                        excepted = Some(links.excepted);
                        DnsStatus::ForcedPlaintext
                    } else {
                        // Encrypted DNS is switched off on the running connection only,
                        // for a sign-in page. Never reported as encrypted.
                        excepted = Some(links.excepted);
                        paused = Some(links.paused.keys().cloned().collect());
                        paused_links = links.paused;
                        DnsStatus::PausedForSignIn
                    }
                } else {
                    DnsStatus::Degraded
                }
            }
            Err(_) => DnsStatus::Error,
        };
        if excepted.is_some() && paused.is_none() {
            paused = Some(BTreeSet::new());
        }
        self.note_pauses(paused);
        if status == DnsStatus::PausedForSignIn {
            self.lift_pause_when_online("nmcli", &paused_links).await;
        }
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

/// Plaintext links found on a check, by NetworkManager connection name.
#[derive(Debug, PartialEq)]
struct PlaintextLinks {
    /// Connections whose saved profile has connection.dns-over-tls "no".
    excepted: BTreeSet<String>,
    /// Connections whose running setting is "no" while the saved profile is
    /// not (a pause for a sign-in page), with the interface name.
    paused: BTreeMap<String, String>,
}

/// The connection names, when the global scope is strict and every non-strict
/// link is either a connection the user exempted (saved connection.dns-over-tls
/// "no") or one paused for a sign-in page (running setting "no"). None in every
/// other case.
async fn classify_plaintext_links(nmcli: &str, probe: &dyn NmProbe, resolvectl: &str) -> Option<PlaintextLinks> {
    let scopes = scopes(resolvectl);
    let global_ok = scopes.iter().any(|s| s.name.is_none() && s.has_dns && s.strict);
    let plain: Vec<_> = scopes.iter().filter(|s| s.has_dns && !s.strict).collect();
    if !global_ok || plain.is_empty() { return None; }
    let mut links = PlaintextLinks { excepted: BTreeSet::new(), paused: BTreeMap::new() };
    for scope in plain {
        let dev = scope.name.as_deref()?;
        let out = crate::command::run(nmcli, &["-g", "GENERAL.CONNECTION", "device", "show", dev]).await.ok()?;
        let conn = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if conn.is_empty() { return None; }
        let out = crate::command::run(nmcli, &["-g", "connection.dns-over-tls", "connection", "show", &conn]).await.ok()?;
        if is_user_exception(&String::from_utf8_lossy(&out.stdout)) {
            links.excepted.insert(conn);
        } else if probe.applied_dns_over_tls(dev).await == Some(0) {
            links.paused.insert(conn, dev.to_string());
        } else {
            return None;
        }
    }
    Some(links)
}

/// The connection names, when the global scope is strict and every non-strict
/// link belongs to a NetworkManager connection the user exempted. None in every
/// other case, including a pause for a sign-in page.
#[cfg(test)]
async fn user_exception_connections(nmcli: &str, resolvectl: &str) -> Option<BTreeSet<String>> {
    struct NoProbe;
    #[async_trait::async_trait]
    impl NmProbe for NoProbe {
        async fn applied_dns_over_tls(&self, _: &str) -> Option<i32> { None }
        async fn device_connectivity(&self, _: &str) -> Option<(u32, u32)> { None }
    }
    let links = classify_plaintext_links(nmcli, &NoProbe, resolvectl).await?;
    links.paused.is_empty().then_some(links.excepted)
}

/// nmcli prints the setting as "no", "0" or "0 (no)" depending on version.
fn is_user_exception(value: &str) -> bool {
    let v = value.trim();
    v == "no" || v == "0" || v.starts_with("0 ")
}

struct Scope { name: Option<String>, strict: bool, has_dns: bool }

/// resolvectl status scopes: Global (name None) and "Link N (ifname)".
fn scopes(text: &str) -> Vec<Scope> {
    let mut out = Vec::new();
    let mut cur: Option<Scope> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "Global" || line.starts_with("Link ") {
            if let Some(s) = cur.take() { out.push(s); }
            let name = line.strip_prefix("Link ").and_then(|l| {
                let (a, b) = (l.find('(')?, l.rfind(')')?);
                Some(l[a + 1..b].to_string())
            });
            cur = Some(Scope { name, strict: false, has_dns: false });
        } else if let Some(s) = cur.as_mut() {
            if let Some(p) = line.strip_prefix("Protocols:") {
                s.strict = p.split_whitespace().any(|v| v == "+DNSOverTLS");
            }
            if let Some(v) = line.strip_prefix("DNS Servers:") {
                s.has_dns = !v.trim().is_empty();
            }
        }
    }
    if let Some(s) = cur { out.push(s); }
    out
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
    #[test]
    fn finds_the_plaintext_link_by_interface_name() {
        let t = "Global\n Protocols: +DNSOverTLS\n DNS Servers: 9.9.9.9#dns.quad9.net\n\
                 Link 3 (wlp2s0)\n Protocols: +DefaultRoute -DNSOverTLS\n DNS Servers: 192.168.1.1\n";
        let s = scopes(t);
        assert_eq!(s.len(), 2);
        assert!(s[0].name.is_none() && s[0].strict && s[0].has_dns);
        assert_eq!(s[1].name.as_deref(), Some("wlp2s0"));
        assert!(!s[1].strict && s[1].has_dns);
    }
    fn set(names: &[&str]) -> Option<BTreeSet<String>> {
        Some(names.iter().map(|n| n.to_string()).collect())
    }

    #[test]
    fn a_connection_name_cannot_add_lines_or_unbounded_text_to_an_event() {
        assert_eq!(shown("Cafe\nfake\u{7}"), "Cafe?fake?");
        let long = shown(&"x".repeat(500));
        assert_eq!(long.chars().count(), 67);
        assert!(long.ends_with("..."));
        assert_eq!(shown("Caf\u{e9} \"Free\""), "Caf\u{e9} \"Free\"");
    }

    const GLOBAL: &str = "Global\n Protocols: +DNSOverTLS\n DNS Servers: 9.9.9.9#dns.quad9.net\n";
    const WIFI: &str = "Link 3 (wlp2s0)\n Protocols: +DefaultRoute -DNSOverTLS\n DNS Servers: 192.168.1.1\n";
    const WIRED: &str = "Link 2 (enp1s0)\n Protocols: +DefaultRoute -DNSOverTLS\n DNS Servers: 10.0.0.1\n";

    /// A stand-in for nmcli: answers the two queries arkad makes from a fixed
    /// table of device, connection name and connection.dns-over-tls value.
    struct FakeNmcli(std::path::PathBuf);
    impl FakeNmcli {
        fn new(table: &[(&str, &str, &str)]) -> Self {
            use std::os::unix::fs::PermissionsExt;
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "arkad-fake-nmcli-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            ));
            // Called as: -g GENERAL.CONNECTION device show DEV
            //        or: -g connection.dns-over-tls connection show NAME
            let mut script = String::from("#!/bin/sh\ncase \"$2|$5\" in\n");
            for (dev, conn, value) in table {
                script.push_str(&format!("'GENERAL.CONNECTION|{dev}') echo '{conn}' ;;\n"));
                script.push_str(&format!("'connection.dns-over-tls|{conn}') echo '{value}' ;;\n"));
            }
            script.push_str("*) exit 10 ;;\nesac\n");
            // Written by a child process. A file this process had open for
            // writing could still be open in a child another test forks at the
            // same moment, and running it would then fail as "text file busy".
            let mut writer = std::process::Command::new("sh")
                .args(["-c", "umask 077 && cat > \"$1\"", "sh"])
                .arg(&path)
                .stdin(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            std::io::Write::write_all(&mut writer.stdin.take().unwrap(), script.as_bytes()).unwrap();
            assert!(writer.wait().unwrap().success());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn path(&self) -> &str { self.0.to_str().unwrap() }
    }
    impl Drop for FakeNmcli {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_file(&self.0) {
                eprintln!("could not remove {}: {e}", self.0.display());
            }
        }
    }

    #[tokio::test]
    async fn exception_connections_are_named_only_when_every_plaintext_link_is_excepted() {
        let both = format!("{GLOBAL}{WIFI}{WIRED}");
        let nm = FakeNmcli::new(&[("wlp2s0", "Cafe Free WiFi", "0 (no)"), ("enp1s0", "Office", "no")]);
        assert_eq!(user_exception_connections(nm.path(), &format!("{GLOBAL}{WIFI}")).await, set(&["Cafe Free WiFi"]));
        assert_eq!(user_exception_connections(nm.path(), &both).await, set(&["Cafe Free WiFi", "Office"]));

        // One link without the exception: not a user exception at all.
        let nm = FakeNmcli::new(&[("wlp2s0", "Cafe Free WiFi", "0 (no)"), ("enp1s0", "Office", "-1 (default)")]);
        assert_eq!(user_exception_connections(nm.path(), &both).await, None);
        // NetworkManager does not know the link, or has no connection on it.
        assert_eq!(user_exception_connections(nm.path(), &format!("{GLOBAL}Link 9 (tun0)\n Protocols: -DNSOverTLS\n DNS Servers: 10.8.0.1\n")).await, None);
        let nm = FakeNmcli::new(&[("wlp2s0", "", "no")]);
        assert_eq!(user_exception_connections(nm.path(), &format!("{GLOBAL}{WIFI}")).await, None);
        // No plaintext link, a plaintext global scope, or no nmcli: never an exception.
        assert_eq!(user_exception_connections(nm.path(), GLOBAL).await, None);
        assert_eq!(user_exception_connections(nm.path(), &format!("{}{WIFI}", GLOBAL.replace("+DNSOverTLS", "-DNSOverTLS"))).await, None);
        assert_eq!(user_exception_connections("/nonexistent/nmcli", &format!("{GLOBAL}{WIFI}")).await, None);
    }

    #[test]
    fn only_an_explicit_no_counts_as_a_user_exception() {
        for v in ["no", "0", "0 (no)", " no\n"] { assert!(is_user_exception(v), "{v}"); }
        for v in ["", "-1", "-1 (default)", "1 (opportunistic)", "2 (yes)", "yes", "default"] {
            assert!(!is_user_exception(v), "{v}");
        }
    }

    // ---- pause for a sign-in page ----

    struct FakeProbe { applied: Vec<(&'static str, i32)>, device: Mutex<Vec<(&'static str, (u32, u32))>> }
    #[async_trait::async_trait]
    impl NmProbe for FakeProbe {
        async fn applied_dns_over_tls(&self, iface: &str) -> Option<i32> {
            self.applied.iter().find(|(d, _)| *d == iface).map(|(_, v)| *v)
        }
        async fn device_connectivity(&self, iface: &str) -> Option<(u32, u32)> {
            self.device.lock().unwrap().iter().find(|(d, _)| *d == iface).map(|(_, v)| *v)
        }
    }

    #[tokio::test]
    async fn a_running_no_over_a_saved_default_is_a_pause_and_a_saved_no_is_an_exception() {
        let text = format!("{GLOBAL}{WIFI}");
        let nm = FakeNmcli::new(&[("wlp2s0", "Cafe Free WiFi", "-1 (default)")]);
        let paused = FakeProbe { applied: vec![("wlp2s0", 0)], device: Mutex::new(vec![]) };
        let links = classify_plaintext_links(nm.path(), &paused, &text).await.unwrap();
        assert!(links.excepted.is_empty());
        assert_eq!(links.paused.get("Cafe Free WiFi").map(String::as_str), Some("wlp2s0"));
        // The old single-purpose view never sees a pause as an exception.
        assert_eq!(user_exception_connections(nm.path(), &text).await, None);

        // Running setting still default, or unreadable: neither (Degraded).
        for applied in [vec![("wlp2s0", -1)], vec![]] {
            let probe = FakeProbe { applied, device: Mutex::new(vec![]) };
            assert_eq!(classify_plaintext_links(nm.path(), &probe, &text).await, None);
        }
        // A saved "no" is an exception whatever the running setting says.
        let nm = FakeNmcli::new(&[("wlp2s0", "Cafe Free WiFi", "no")]);
        let probe = FakeProbe { applied: vec![("wlp2s0", 0)], device: Mutex::new(vec![]) };
        let links = classify_plaintext_links(nm.path(), &probe, &text).await.unwrap();
        assert!(links.paused.is_empty() && links.excepted.contains("Cafe Free WiFi"));
        // One paused link and one plaintext link that is neither: not a pause.
        let nm = FakeNmcli::new(&[("wlp2s0", "Cafe Free WiFi", "-1"), ("enp1s0", "Office", "-1")]);
        let probe = FakeProbe { applied: vec![("wlp2s0", 0), ("enp1s0", -1)], device: Mutex::new(vec![]) };
        assert_eq!(classify_plaintext_links(nm.path(), &probe, &format!("{GLOBAL}{WIFI}{WIRED}")).await, None);
    }

    fn names(n: &[&str]) -> Option<BTreeSet<String>> { set(n) }

    #[test]
    fn a_pause_is_logged_when_it_starts_and_when_it_ends_and_not_in_between() {
        let mut t = PauseTracker::default();
        assert!(t.observe(names(&[])).is_empty());
        let started = t.observe(names(&["Cafe"]));
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].0, "paused");
        assert!(started[0].1.contains("\"Cafe\""));
        for _ in 0..5 { assert!(t.observe(names(&["Cafe"])).is_empty()); }
        assert!(t.observe(None).is_empty(), "an unclear check is not an end");
        let ended = t.observe(names(&[]));
        assert_eq!(ended, [("paused-ended", "Encrypted DNS is no longer paused on the connection \"Cafe\"".to_string())]);
        assert!(t.observe(names(&[])).is_empty());
    }

    #[test]
    fn two_full_access_checks_in_a_row_lift_the_pause() {
        let t0 = Instant::now();
        let step = Duration::from_secs(60);
        let mut t = PauseTracker::default();
        // Not full: never.
        for i in 0..4 { assert!(!t.note_full("Cafe", false, t0 + i * step)); }
        // Full once, then not full: the count starts over.
        assert!(!t.note_full("Cafe", true, t0));
        assert!(!t.note_full("Cafe", false, t0 + step));
        assert!(!t.note_full("Cafe", true, t0 + 2 * step));
        assert!(t.note_full("Cafe", true, t0 + 3 * step), "second full check in a row");
        // After lifting, it takes two more.
        assert!(!t.note_full("Cafe", true, t0 + 4 * step));
        assert!(t.note_full("Cafe", true, t0 + 5 * step));
    }

    #[test]
    fn two_readings_within_one_reconcile_count_once() {
        let t0 = Instant::now();
        let mut t = PauseTracker::default();
        assert!(!t.note_full("Cafe", true, t0));
        assert!(!t.note_full("Cafe", true, t0 + Duration::from_secs(1)));
        assert!(t.note_full("Cafe", true, t0 + Duration::from_secs(60)));
    }

    #[test]
    fn each_paused_connection_is_counted_on_its_own() {
        let t0 = Instant::now();
        let mut t = PauseTracker::default();
        assert!(!t.note_full("Cafe", true, t0));
        assert!(!t.note_full("Hotel", true, t0 + Duration::from_secs(60)), "Hotel's first check");
        assert!(t.note_full("Cafe", true, t0 + Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn the_backstop_reapplies_the_paused_interface_and_logs_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("arkad-backstop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("calls");
        let script = dir.join("nmcli");
        let mut w = std::process::Command::new("sh")
            .args(["-c", "umask 077 && cat > \"$1\"", "sh"]).arg(&script)
            .stdin(std::process::Stdio::piped()).spawn().unwrap();
        std::io::Write::write_all(&mut w.stdin.take().unwrap(),
            format!("#!/bin/sh\necho \"$@\" >> '{}'\n", record.display()).as_bytes()).unwrap();
        assert!(w.wait().unwrap().success());
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();

        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        // Ethernet has full access; the paused Wi-Fi is still at its sign-in page.
        let probe = Arc::new(FakeProbe { applied: vec![], device: Mutex::new(vec![("enp1s0", (4, 4)), ("wlp2s0", (2, 0))]) });
        let device = probe.clone();
        let e = DnsEnforcer::with_log_and_probe(&Config::default(), Arc::new(move |c, ev, m| {
            sink.lock().unwrap().push(format!("{c}|{ev}|{m}"));
        }), probe);
        let paused: BTreeMap<String, String> = [("Cafe".to_string(), "wlp2s0".to_string())].into();
        for _ in 0..3 {
            e.pauses.lock().unwrap().full.clear();
            e.lift_pause_when_online(script.to_str().unwrap(), &paused).await;
        }
        assert!(!record.exists(), "another link being FULL never restores the paused one");
        // Now the paused device itself is FULL on IPv6 only: still counts.
        *device.device.lock().unwrap() = vec![("wlp2s0", (2, 4))];
        e.lift_pause_when_online(script.to_str().unwrap(), &paused).await;
        assert!(!record.exists(), "one FULL check is not enough");
        // The gap between counted checks is 20 s; backdate the first one.
        e.pauses.lock().unwrap().full.get_mut("Cafe").unwrap().1 = Instant::now() - Duration::from_secs(60);
        e.lift_pause_when_online(script.to_str().unwrap(), &paused).await;
        assert_eq!(std::fs::read_to_string(&record).unwrap().trim(), "device reapply wlp2s0");
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(events[0].starts_with("dns|paused-restored|") && events[0].contains("\"Cafe\""));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
