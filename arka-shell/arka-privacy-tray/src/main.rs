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

use privacy::{drifts_since, level, parse_log, Enforcement, Event, Level, Notification, Notifier};

use ksni::menu::{MenuItem, StandardItem};
use ksni::{Tray, TrayMethods};
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_PATH: &str = "/var/log/arkaos/privacy.jsonl";
const POLL_SECS: u64 = 4;
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
            return "Privacy daemon not reachable".to_string();
        }
        self.level.header(self.drifts_today)
    }
}

impl Tray for ArkaTray {
    fn id(&self) -> String {
        "org.arka.privacy-tray".into()
    }

    fn title(&self) -> String {
        "ArkaOS Privacy".into()
    }

    fn icon_name(&self) -> String {
        if !self.arkad_up {
            return "security-low".into();
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
                        enabled: false,
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
    StandardItem { label, icon_name, enabled: false, ..Default::default() }.into()
}

// ── arkad (system bus, read-only) ────────────────────────────────────────────

#[zbus::proxy(
    interface = "org.arka.arkad",
    default_service = "org.arka.arkad",
    default_path = "/org/arka/arkad",
    gen_blocking = false
)]
trait Arkad {
    #[zbus(property)]
    fn mac_randomization(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn dns_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn hostname_privacy(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn ipv6_privacy(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn browser_sandbox(&self) -> zbus::Result<String>;
}

/// A string status is "active" unless arkad reports it as Unknown/empty.
fn active_str(s: &str) -> bool {
    !s.is_empty() && !s.starts_with("Unknown") && !s.eq_ignore_ascii_case("unknown")
}

async fn read_enforcement(p: &ArkadProxy<'_>) -> Option<Enforcement> {
    // If the first read fails, arkad isn't up — report None so the tray can say so.
    let mac = p.mac_randomization().await.ok()?;
    let dns = p.dns_status().await.unwrap_or_else(|_| "Unknown".into());
    let hostname = p.hostname_privacy().await.unwrap_or(false);
    let ipv6 = p.ipv6_privacy().await.unwrap_or(false);
    let browser = p.browser_sandbox().await.unwrap_or_else(|_| "Unknown".into());
    Some(Enforcement {
        mac,
        dns_ok: active_str(&dns),
        dns_detail: if active_str(&dns) { format!("Encrypted — {dns}") } else { dns },
        hostname,
        ipv6,
        browser_ok: active_str(&browser),
        browser_detail: if active_str(&browser) {
            format!("Sandboxed — {browser}")
        } else {
            "Not sandboxed".into()
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

// ── main loop ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).without_time().init();

    let handle = match ArkaTray::default().spawn().await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("could not register tray item: {e}");
            std::process::exit(1);
        }
    };

    let sys = zbus::Connection::system().await.ok();
    let arkad = match &sys {
        Some(c) => ArkadProxy::new(c).await.ok(),
        None => None,
    };
    let ses = zbus::Connection::session().await.ok();
    let notifier_proxy = match &ses {
        Some(c) => NotificationsProxy::new(c).await.ok(),
        None => None,
    };

    // start_ts = now so pre-existing log lines from earlier boots never fire.
    let mut notifier = Notifier::new(NOTIFY_MIN_GAP_SECS, now());

    loop {
        let t = now();
        let enf = match &arkad {
            Some(p) => read_enforcement(p).await,
            None => None,
        };
        let events = read_events();
        let drifts_today = drifts_since(&events, utc_midnight(t));
        let recent_drift = events.iter().any(|e| e.is_drift() && e.ts >= t.saturating_sub(RECENT_DRIFT_WINDOW));

        let (enf_now, arkad_up) = match enf {
            Some(e) => (e, true),
            None => (Enforcement::default(), false),
        };
        let lvl = if arkad_up { level(&enf_now, recent_drift) } else { Level::Critical };

        // Notify (only when arkad is reachable — otherwise state is unknown, not "down").
        if arkad_up {
            if let (Some(np), Some(n)) = (&notifier_proxy, notifier.evaluate(&events, &enf_now, t)) {
                send_notification(np, &n).await;
            }
        }

        let recent: Vec<Event> = events;
        handle
            .update(move |tray: &mut ArkaTray| {
                tray.enf = enf_now;
                tray.level = lvl;
                tray.drifts_today = drifts_today;
                tray.recent = recent;
                tray.arkad_up = arkad_up;
            })
            .await;

        tokio::time::sleep(std::time::Duration::from_secs(POLL_SECS)).await;
    }
}
