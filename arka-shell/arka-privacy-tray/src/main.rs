//! arka-privacy-tray — "Privacy Activity" StatusNotifierItem for ArkaOS.
//!
//! Makes arkad's enforcement visible in the first 60 seconds, ambiently and
//! honestly. It is strictly read-only:
//!   * enforcement STATE  ← arkad D-Bus properties (org.arka.arkad, system bus)
//!   * notable EVENTS     ← arkad's world-readable /var/log/arkaos/privacy.jsonl
//! It never blocks, never prompts modally, and never logs per-connection or
//! per-domain traffic (that is phase 2 — docs/PHASE2-LOGGING-SCOPE.md).
//!
//! All trust-critical decisions (icon level, panel rows, drift counting,
//! notification tiering + rate-limiting) live in the unit-tested `privacy`
//! module; this file is just the D-Bus/Plasma binding around it.

mod privacy;

use arka_shell_common::DnsStatus;
use privacy::{
    browser_active, check_is_fresh, dns_active, dns_paused, dns_user_off, drifts_since, level, parse_log, pause_args,
    device_is_full, portal_action, portal_offer, reapply_args, sign_in_uri, Enforcement, Event, Level, Notification, Notifier,
    PortalAction, ACTION_PAUSE,
};

use ksni::menu::{MenuItem, StandardItem};
use ksni::{Tray, TrayMethods};
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_PATH: &str = "/var/log/arkaos/privacy.jsonl";
// arkad normally reconciles every 60 seconds; expire after three polls.
const POLL_SECS: u64 = 30;
const EVENT_CAP: usize = 60;
const RECENT_SHOWN: usize = 4;
const NOTIFY_MIN_GAP_SECS: u64 = 600; // ≥10 min between notifications — anti-fatigue
const RECENT_DRIFT_WINDOW: u64 = 6 * 3600; // "self-healed" state lingers 6h

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// UTC midnight for the current day — the "today" boundary for drift counting.
/// (Local-tz precision would need a tz dependency; UTC is honest and close.)
fn utc_midnight(now: u64) -> u64 {
    now - (now % 86_400)
}

// ── the tray item ──────────────────────────────────────────────────────────

#[derive(Default)]
struct ArkaTray {
    enf: Enforcement,
    level: Level,
    drifts_today: usize,
    recent: Vec<Event>,
    arkad_up: bool,
}

impl ArkaTray {
    fn header(&self) -> String {
        if !self.arkad_up {
            return "Status unknown — no fresh privacy check".to_string();
        }
        if self.level == Level::Warn && self.enf.dns_user_off {
            return "DNS isn't encrypted on this network — you allowed it".to_string();
        }
        if self.level == Level::Warn && self.enf.dns_paused {
            return "Encrypted DNS is paused for this network's sign-in page".to_string();
        }
        self.level.header(self.drifts_today)
    }
}

impl Tray for ArkaTray {
    // A click on the shield opens the status menu; without this only a right-click did.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "org.arka.privacy-tray".into()
    }

    fn title(&self) -> String {
        "ArkaOS Privacy".into()
    }

    fn icon_name(&self) -> String {
        if !self.arkad_up {
            return "dialog-question-symbolic".into();
        }
        self.level.icon_name().into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "ArkaOS Privacy".into(),
            description: self.header(),
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        // Header — the positive-default state line.
        items.push(info_row(self.header(), String::new()));

        if self.arkad_up {
            items.push(MenuItem::Separator);
            for r in self.enf.rows() {
                let icon = if r.active { "emblem-ok-symbolic" } else { "emblem-important-symbolic" };
                items.push(
                    StandardItem {
                        label: format!("{}  —  {}", r.name, r.detail),
                        icon_name: icon.into(),
                        ..Default::default()
                    }
                    .into(),
                );
            }

            // Recent notable events (newest first), if any.
            let shown: Vec<&Event> = self.recent.iter().rev().take(RECENT_SHOWN).collect();
            if !shown.is_empty() {
                items.push(MenuItem::Separator);
                items.push(info_row("Recent".to_string(), String::new()));
                for e in shown {
                    items.push(info_row(format!("• {}", e.msg), String::new()));
                }
            }
        }

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Open Privacy Dashboard".into(),
                icon_name: "security-high".into(),
                activate: Box::new(|_: &mut Self| {
                    let _ = std::process::Command::new("arka-dashboard").spawn();
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

/// A non-interactive info line in the menu.
fn info_row(label: String, icon_name: String) -> MenuItem<ArkaTray> {
    // Not disabled: a disabled row is drawn greyed out at about 2.5:1 contrast and looks broken. It
    // stays non-interactive because the default action does nothing.
    StandardItem { label, icon_name, ..Default::default() }.into()
}

// ── arkad (system bus, read-only) ────────────────────────────────────────────

#[zbus::proxy(
    interface = "org.arka.arkad",
    default_service = "org.arka.arkad",
    default_path = "/org/arka/arkad",
    gen_blocking = false
)]
trait Arkad {
    #[zbus(property(emits_changed_signal = "false"))]
    fn last_check(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn mac_randomization(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn dns_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn hostname_privacy(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn ipv6_privacy(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn firewall(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn browser_sandbox(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn sandbox_status(&self) -> zbus::Result<String>;
}

async fn read_enforcement(p: &ArkadProxy<'_>) -> Option<Enforcement> {
    let last_check = p.last_check().await.ok()?;
    if !check_is_fresh(Some(last_check), now(), POLL_SECS) { return None; }
    // If the first read fails, arkad isn't up — report None so the tray can say so.
    let mac = p.mac_randomization().await.ok()?;
    let dns = p.dns_status().await.unwrap_or_else(|_| "Unknown".into());
    let hostname = p.hostname_privacy().await.unwrap_or(false);
    let ipv6 = p.ipv6_privacy().await.unwrap_or(false);
    let firewall = p.firewall().await.ok()?;
    let browser = p.browser_sandbox().await.unwrap_or_else(|_| "Unknown".into());
    let sandbox = p.sandbox_status().await.unwrap_or_else(|_| "Unknown".into());
    // A slow read or an intervening reconcile must not bless an old snapshot.
    if p.last_check().await.ok()? != last_check
        || !check_is_fresh(Some(last_check), now(), POLL_SECS) { return None; }
    let dns_ok = dns_active(&dns);
    let dns_off_by_choice = dns_user_off(&dns);
    let dns_is_paused = dns_paused(&dns);
    let browser_ok = browser_active(&browser, &sandbox);
    Some(Enforcement {
        mac,
        dns_ok,
        dns_user_off: dns_off_by_choice,
        dns_paused: dns_is_paused,
        dns_detail: dns,
        hostname,
        ipv6,
        firewall,
        browser_ok,
        browser_detail: if browser_ok {
            "Firefox sees only your Downloads folder and files you have picked in the file dialog".to_string()
        } else {
            "Not confirmed".to_string()
        },
    })
}

fn read_events() -> Vec<Event> {
    match std::fs::read_to_string(LOG_PATH) {
        Ok(body) => parse_log(&body, EVENT_CAP),
        Err(_) => Vec::new(),
    }
}

// ── desktop notifications (session bus) ──────────────────────────────────────

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications",
    gen_blocking = false
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    fn close_notification(&self, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;
}

async fn send_notification(p: &NotificationsProxy<'_>, n: &Notification) {
    let mut hints = std::collections::HashMap::new();
    // urgency: 0 low, 1 normal, 2 critical
    let urgency: u8 = if n.critical { 2 } else { 1 };
    hints.insert("urgency", zbus::zvariant::Value::U8(urgency));
    let icon = if n.critical { "security-low" } else { "security-medium" };
    let _ = p
        .notify("ArkaOS Privacy", 0, icon, &n.summary, &n.body, &[], hints, 8000)
        .await;
}


// ── sign-in pages (captive portals) ──────────────────────────────────────────
//
// NetworkManager reports Connectivity PORTAL (2) when a network answers its
// check with a sign-in page. The tray then offers, once per activation, to use
// the network's own DNS until the sign-in is done. It changes only the running
// connection (`nmcli device modify`) and puts it back with `nmcli device
// reapply` when Connectivity is FULL (4). The decisions are in `privacy`.

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    gen_blocking = false
)]
trait NmManager {
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn connectivity_check_uri(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    fn get_device_by_ip_iface(&self, iface: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager",
    gen_blocking = false
)]
trait NmActive {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Type")]
    fn connection_type(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn vpn(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager",
    gen_blocking = false
)]
trait NmDevice {
    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn ip4_connectivity(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn ip6_connectivity(&self) -> zbus::Result<u32>;
}

/// The connection the machine is currently using for its default route.
struct Primary {
    /// D-Bus path of the active connection: one value per activation.
    activation: String,
    name: String,
    iface: String,
    is_vpn: bool,
}

async fn read_primary(sys: &zbus::Connection) -> Option<Primary> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mgr = NmManagerProxy::builder(sys).cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        let path = mgr.primary_connection().await.ok()?;
        if path.as_str() == "/" { return None; }
        let activation = path.as_str().to_string();
        let act = NmActiveProxy::builder(sys).path(path).ok()?
            .cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        let kind = act.connection_type().await.ok()?;
        let is_vpn = act.vpn().await.unwrap_or(false) || kind == "vpn" || kind == "wireguard";
        let dev = act.devices().await.ok()?.into_iter().next()?;
        let dev = NmDeviceProxy::builder(sys).path(dev).ok()?
            .cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        Some(Primary { activation, name: act.id().await.ok()?, iface: dev.interface().await.ok()?, is_vpn })
    }).await.ok().flatten()
}

/// The paused interface's own (Ip4Connectivity, Ip6Connectivity). None when unreadable.
async fn read_device_connectivity(sys: &zbus::Connection, iface: &str) -> Option<(u32, u32)> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mgr = NmManagerProxy::builder(sys).cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        let path = mgr.get_device_by_ip_iface(iface).await.ok()?;
        let dev = NmDeviceProxy::builder(sys).path(path).ok()?
            .cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        Some((dev.ip4_connectivity().await.ok()?, dev.ip6_connectivity().await.ok()?))
    }).await.ok().flatten()
}

/// arkad's DNS status right now (not the 30 s snapshot), Unknown when unreadable.
async fn read_dns_status(sys: &zbus::Connection) -> DnsStatus {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let proxy = ArkadProxy::builder(sys).cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        proxy.dns_status().await.ok()
    }).await.ok().flatten().map(DnsStatus::from).unwrap_or_default()
}

async fn run_nmcli(args: Vec<String>) -> bool {
    let child = tokio::process::Command::new("nmcli").args(&args).env("LC_ALL", "C").kill_on_drop(true).status();
    match tokio::time::timeout(std::time::Duration::from_secs(10), child).await {
        Ok(Ok(status)) => status.success(),
        _ => false,
    }
}

async fn send_offer(p: &NotificationsProxy<'_>, connection: &str) -> Option<u32> {
    let offer = portal_offer(connection);
    let actions: Vec<&str> = offer.actions.iter().flat_map(|(k, l)| [*k, *l]).collect();
    let mut hints = std::collections::HashMap::new();
    hints.insert("urgency", zbus::zvariant::Value::U8(1));
    p.notify("Arka Privacy", 0, "network-wireless", &offer.summary, &offer.body, &actions, hints, 0).await.ok()
}

async fn portal_watch(sys: zbus::Connection, ses: zbus::Connection) {
    use futures_util::StreamExt;
    let (Ok(mgr), Ok(np)) = (NmManagerProxy::new(&sys).await, NotificationsProxy::new(&ses).await) else { return };
    let Ok(mut actions) = np.receive_action_invoked().await else { return };
    let mut changes = mgr.receive_connectivity_changed().await;
    // The offer on screen: (notification id, interface to pause).
    let mut offer: Option<(u32, String)> = None;
    // The activation the last offer was made for: one offer per activation.
    let mut offered_for: Option<String> = None;
    // An interface this tray paused. Restoring it waits for that device's own
    // full access, which is polled while it is paused (the Manager's overall
    // value may already be FULL through another link).
    let mut paused_iface: Option<String> = None;
    let mut pending = mgr.connectivity().await.ok();
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let mut new = None;
        if let Some(v) = pending.take() {
            new = Some(v);
        } else {
            tokio::select! {
                Some(change) = changes.next() => match change.get().await { Ok(v) => new = Some(v), Err(_) => continue },
                Some(signal) = actions.next() => {
                    let Ok(args) = signal.args() else { continue };
                    let Some((id, iface)) = offer.clone() else { continue };
                    if args.id != id { continue }
                    offer = None;
                    if args.action_key == ACTION_PAUSE && run_nmcli(pause_args(&iface)).await {
                        paused_iface = Some(iface);
                        let uri = mgr.connectivity_check_uri().await.unwrap_or_default();
                        if let Some(uri) = sign_in_uri(&uri) {
                            let _ = tokio::process::Command::new("xdg-open").arg(uri).spawn();
                        }
                    }
                    continue
                },
                _ = tick.tick(), if paused_iface.is_some() => {}
                else => return,
            }
        }

        if let Some(new) = new {
            let dns = read_dns_status(&sys).await;
            let primary = read_primary(&sys).await;
            if let Some(p) = primary.as_ref() {
                let action = portal_action(new, &dns, p.is_vpn, &p.activation, offered_for.as_deref());
                if action == PortalAction::Offer {
                    if let Some(id) = send_offer(&np, &p.name).await {
                        if let Some((old, _)) = offer.replace((id, p.iface.clone())) { let _ = np.close_notification(old).await; }
                        offered_for = Some(p.activation.clone());
                    }
                }
            }
            if new == privacy::CONNECTIVITY_FULL { offered_for = None; }
            // Drop the offer together with its notification (it has no timeout), so no dead button stays on screen.
            if new != privacy::CONNECTIVITY_PORTAL {
                if let Some((id, _)) = offer.take() { let _ = np.close_notification(id).await; }
            }
            // Every link is down: the pause ended with it.
            if new <= 1 { paused_iface = None; }
            // Paused by a tray that was restarted, or by something else: arkad says so.
            if paused_iface.is_none() && dns == DnsStatus::PausedForSignIn {
                paused_iface = primary.map(|p| p.iface);
            }
        }

        // Restore: the interface this tray paused wins over whatever is primary now.
        if let Some(iface) = paused_iface.clone() {
            if let Some((ip4, ip6)) = read_device_connectivity(&sys, &iface).await {
                if device_is_full(ip4, ip6) {
                    run_nmcli(reapply_args(&iface)).await;
                    paused_iface = None;
                }
            }
        }
    }
}

// ── main loop ────────────────────────────────────────────────────────────────

/// One reading of arkad and the event log: what the tray shows right now.
async fn snapshot(t: u64) -> (ArkaTray, Vec<Event>) {
    let enf = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let sys = zbus::Connection::system().await.ok()?;
        let proxy = ArkadProxy::builder(&sys)
            .cache_properties(zbus::proxy::CacheProperties::No).build().await.ok()?;
        read_enforcement(&proxy).await
    }).await.ok().flatten();
    let events = read_events();
    let drifts_today = drifts_since(&events, utc_midnight(t));
    let recent_drift = events.iter().any(|e| e.is_drift() && e.ts >= t.saturating_sub(RECENT_DRIFT_WINDOW));

    let (enf_now, arkad_up) = match enf {
        Some(e) => (e, true),
        None => (Enforcement::default(), false),
    };
    let lvl = if arkad_up { level(&enf_now, recent_drift) } else { Level::Critical };
    let tray = ArkaTray { enf: enf_now, level: lvl, drifts_today, recent: events.clone(), arkad_up };
    (tray, events)
}

/// `--status`: print the tray's level and header once, for the launcher.
fn status_line(tray: &ArkaTray) -> String {
    let lvl = if !tray.arkad_up {
        "unknown"
    } else {
        match tray.level {
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Critical => "critical",
        }
    };
    format!("{lvl}\t{}", tray.header())
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("--status") {
        let (tray, _) = snapshot(now()).await;
        println!("{}", status_line(&tray));
        return;
    }

    tracing_subscriber::fmt().with_target(false).without_time().init();

    let handle = match ArkaTray::default().spawn().await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("could not register tray item: {e}");
            std::process::exit(1);
        }
    };

    let ses = zbus::Connection::session().await.ok();
    let notifier_proxy = match &ses {
        Some(c) => NotificationsProxy::new(c).await.ok(),
        None => None,
    };

    if let (Some(ses), Ok(sys)) = (ses.clone(), zbus::Connection::system().await) {
        tokio::spawn(portal_watch(sys, ses));
    }

    // start_ts = now so pre-existing log lines from earlier boots never fire.
    let mut notifier = Notifier::new(NOTIFY_MIN_GAP_SECS, now());

    loop {
        let t = now();
        let (next, events) = snapshot(t).await;

        // Notify (only when arkad is reachable — otherwise state is unknown, not "down").
        if next.arkad_up {
            if let (Some(np), Some(n)) = (&notifier_proxy, notifier.evaluate(&events, &next.enf, t)) {
                send_notification(np, &n).await;
            }
        }

        handle
            .update(move |tray: &mut ArkaTray| *tray = next)
            .await;

        tokio::time::sleep(std::time::Duration::from_secs(POLL_SECS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_status_hides_positive_rows_and_uses_neutral_icon() {
        let tray = ArkaTray::default();
        assert_eq!(tray.icon_name(), "dialog-question-symbolic");
        assert!(tray.header().starts_with("Status unknown"));
        assert_eq!(tray.menu().len(), 3);
    }

    #[test]
    fn status_line_never_reports_ok_without_a_fresh_check() {
        let tray = ArkaTray::default();
        assert!(status_line(&tray).starts_with("unknown\tStatus unknown"));
        let up = ArkaTray { arkad_up: true, ..ArkaTray::default() };
        assert_eq!(status_line(&up), "ok\tPrivacy settings are in place");
    }
}
