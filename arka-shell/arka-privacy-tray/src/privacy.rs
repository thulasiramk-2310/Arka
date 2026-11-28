//! Pure, binding-agnostic core for the Privacy Activity tray.
//!
//! No D-Bus and no GUI live here — only the honest state/event model and the
//! rules that decide (a) the tray icon level, (b) the panel rows, (c) how many
//! drifts happened today, and (d) whether a new event is worth a notification.
//! Keeping this pure makes the trust-critical behaviour unit-testable without a
//! running Plasma session.
//!
//! Honesty rules baked in (the project's standing honesty rules):
//!   * We report enforcement STATE and NOTABLE EVENTS — never per-connection or
//!     per-domain traffic (that would be a surveillance layer; phase 2 only).
//!   * The positive default is silence: "Privacy settings are in place".
//!   * Only a real, new drift (warn) or a protection currently DOWN (critical)
//!     may notify. Routine/info events never do. Notifications are rate-limited
//!     and coalesced so warning-fatigue can't set in.

use serde::Deserialize;
use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};

pub fn dns_active(value: &str) -> bool {
    DnsStatus::from(value).is_encrypted()
}

/// The user allowed unencrypted DNS on the current network (arkad reports
/// ForcedPlaintext only for an explicit per-network exception).
pub fn dns_user_off(value: &str) -> bool {
    DnsStatus::from(value) == DnsStatus::ForcedPlaintext
}

/// Encrypted DNS is switched off on the running connection only, for the
/// network's sign-in page (arkad reports PausedForSignIn).
pub fn dns_paused(value: &str) -> bool {
    DnsStatus::from(value) == DnsStatus::PausedForSignIn
}

pub fn browser_active(mode: &str, sandbox: &str) -> bool {
    BrowserSandbox::from(mode).is_isolated() && SandboxStatus::from(sandbox).is_active()
}

/// Live snapshot of arkad's enforcement, read from its D-Bus properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Enforcement {
    pub mac: bool,
    pub dns_detail: String,
    pub dns_ok: bool,
    /// DNS is unencrypted because the user chose that for this network.
    pub dns_user_off: bool,
    /// Encrypted DNS is paused for this network's sign-in page.
    pub dns_paused: bool,
    pub hostname: bool,
    pub ipv6: bool,
    pub firewall: bool,
    pub browser_detail: String,
    pub browser_ok: bool,
}

/// One row in the tray panel: a protection and its live status.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: &'static str,
    pub detail: String,
    pub active: bool,
}

impl Enforcement {
    /// The six setting rows, in a stable order.
    pub fn rows(&self) -> Vec<Row> {
        vec![
            Row { name: "Private network address", detail: if self.mac { "Random on each network".into() } else { "Not random".into() }, active: self.mac },
            Row {
                name: "Encrypted DNS",
                detail: if self.dns_user_off {
                    "Unencrypted on this network (your choice)".into()
                } else if self.dns_paused {
                    "Paused for this network's sign-in page".into()
                } else {
                    self.dns_detail.clone()
                },
                active: self.dns_ok,
            },
            Row { name: "Computer name", detail: if self.hostname { "Kept as chosen in setup".into() } else { "Not as chosen in setup".into() }, active: self.hostname },
            Row { name: "Temporary IPv6 addresses", detail: if self.ipv6 { "On".into() } else { "Off".into() }, active: self.ipv6 },
            Row { name: "Sandboxed browser", detail: self.browser_detail.clone(), active: self.browser_ok },
            Row { name: "Firewall", detail: if self.firewall { "Rejects incoming connections".into() } else { "Not confirmed".into() }, active: self.firewall },
        ]
    }

    /// True only when every protection is currently active.
    pub fn all_active(&self) -> bool {
        self.mac && self.dns_ok && self.hostname && self.ipv6 && self.browser_ok && self.firewall
    }

    /// Settings that are NOT in place and that the user didn't choose to turn
    /// off (for a critical alert).
    pub fn down(&self) -> Vec<&'static str> {
        self.rows()
            .into_iter()
            .filter(|r| !r.active && !(r.name == "Encrypted DNS" && (self.dns_user_off || self.dns_paused)))
            .map(|r| r.name)
            .collect()
    }
}

/// Overall state, driving the tray icon and header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Ok,       // all protections active, no recent drift
    Warn,     // all active now, but a drift was auto-corrected recently
    Critical, // a protection is currently down
}

impl Level {
    /// Freedesktop-standard themed icon name (Plasma ships these); a distinct
    /// Arka shield glyph is an item-3 (theme) refinement, deliberately not here.
    pub fn icon_name(self) -> &'static str {
        match self {
            Level::Ok => "security-high",
            Level::Warn => "security-medium",
            Level::Critical => "security-low",
        }
    }
    pub fn header(self, drifts_today: usize) -> String {
        match self {
            Level::Ok if drifts_today == 0 => "Privacy settings are in place".to_string(),
            Level::Ok => format!("Privacy settings are in place, {drifts_today} put back today"),
            Level::Warn => format!("A privacy setting was put back, {drifts_today} today"),
            Level::Critical => "A privacy setting is off".to_string(),
        }
    }
}

/// Fail closed on missing/zero, future (clock rollback), or expired timestamps.
/// Freshness describes the check, not whether any control succeeded.
pub fn check_is_fresh(last_check: Option<u64>, now: u64, poll_secs: u64) -> bool {
    last_check.filter(|&last| last != 0)
        .and_then(|last| now.checked_sub(last))
        .is_some_and(|age| age < poll_secs.saturating_mul(3))
}

fn plural(n: usize) -> &'static str { if n == 1 { "" } else { "s" } }

/// Decide the tray level from live enforcement plus whether a drift happened
/// recently. A protection currently down always wins (critical).
pub fn level(enf: &Enforcement, recent_drift: bool) -> Level {
    if !enf.down().is_empty() {
        Level::Critical
    } else if !enf.all_active() || recent_drift {
        // Something is off by the user's own choice, or a drift was corrected.
        Level::Warn
    } else {
        Level::Ok
    }
}

/// One structured event as arkad writes it to /var/log/arkaos/privacy.jsonl.
/// The writer emits `{"ts":N,"cat":"..","ev":"..","msg":".."}` per line.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Event {
    pub ts: u64,
    pub cat: String,
    pub ev: String,
    pub msg: String,
}

impl Event {
    /// Info-level events never notify (started/ready/recovered/etc.).
    pub fn is_drift(&self) -> bool {
        self.ev == "drift"
    }
}

/// Parse one JSONL line into an Event, tolerating blank/garbled lines.
pub fn parse_line(line: &str) -> Option<Event> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str::<Event>(line).ok()
}

/// Parse a whole log body, keeping only the most recent `cap` events.
pub fn parse_log(body: &str, cap: usize) -> Vec<Event> {
    let mut all: Vec<Event> = body.lines().filter_map(parse_line).collect();
    if all.len() > cap {
        all.drain(0..all.len() - cap);
    }
    all
}

/// Number of drift events at or after `since_ts` (used for "N drifts today"
/// with `since_ts` = local midnight).
pub fn drifts_since(events: &[Event], since_ts: u64) -> usize {
    events.iter().filter(|e| e.is_drift() && e.ts >= since_ts).count()
}

/// A notification the tray should raise.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub summary: String,
    pub body: String,
    pub critical: bool,
}

/// Decides, honestly and sparingly, when to raise a desktop notification.
///
/// * Only new drift events (warn) and a protection that is currently down
///   (critical) qualify. Info events are ignored entirely.
/// * At most one notification per `min_gap_secs`; extra events coalesce.
/// * `last_seen_ts` guarantees an event is considered at most once, so a
///   restart re-reading the log can't re-fire old notifications.
pub struct Notifier {
    pub min_gap_secs: u64,
    last_fire: Option<u64>,
    last_seen_ts: u64,
    /// When the current "something is down" episode started, if one is open.
    /// Critical fires once per episode, and only after it has lasted
    /// DOWN_CONFIRM_SECS: a setting that heals by the next check (a slow
    /// firewalld right after boot) is not worth an alarm.
    down_since: Option<u64>,
    fired_this_episode: bool,
}

/// Two tray polls apart (POLL_SECS is 30), with a little slack.
pub const DOWN_CONFIRM_SECS: u64 = 25;

impl Notifier {
    pub fn new(min_gap_secs: u64, start_ts: u64) -> Self {
        Self { min_gap_secs, last_fire: None, last_seen_ts: start_ts, down_since: None, fired_this_episode: false }
    }

    fn rate_ok(&self, now: u64) -> bool {
        match self.last_fire {
            Some(t) => now.saturating_sub(t) >= self.min_gap_secs,
            None => true,
        }
    }

    /// Evaluate one tick. `events` is the full recent log; `enf` is the live
    /// state; `now` is the current unix time. Returns a notification to raise,
    /// or None. Mutates internal bookkeeping either way.
    pub fn evaluate(&mut self, events: &[Event], enf: &Enforcement, now: u64) -> Option<Notification> {
        // Critical: a protection is down and has stayed down across two checks.
        // Fire once per episode and respect the rate limit.
        let down = enf.down();
        let is_down = !down.is_empty();
        if !is_down {
            self.down_since = None;
            self.fired_this_episode = false;
        } else if self.down_since.is_none() {
            self.down_since = Some(now);
        }
        let lasted = self.down_since.is_some_and(|s| now.saturating_sub(s) >= DOWN_CONFIRM_SECS);
        let critical = if is_down && lasted && !self.fired_this_episode && self.rate_ok(now) {
            Some(Notification {
                summary: "A privacy setting isn't in place".to_string(),
                body: format!("Not in place: {}. arkad will try to apply it again.", down.join(", ")),
                critical: true,
            })
        } else {
            None
        };
        if let Some(n) = critical {
            self.fired_this_episode = true;
            self.last_fire = Some(now);
            // still advance last_seen so drift events up to now aren't re-counted
            self.advance_seen(events);
            return Some(n);
        }

        // Warn: new drift events since we last looked, once they are
        // DOWN_CONFIRM_SECS old and still not fixed. arkad often repairs a
        // drift within the same second (an interface coming back after
        // sleep); a popup for something already fixed is only noise. The
        // tray icon still shows "fixed recently" either way.
        let settled = now.saturating_sub(DOWN_CONFIRM_SECS);
        let fresh: Vec<&Event> = events.iter()
            .filter(|e| e.is_drift() && e.ts > self.last_seen_ts && e.ts <= settled)
            .filter(|d| !events.iter().any(|r| r.ev == "recovered" && r.cat == d.cat && r.ts >= d.ts))
            .collect();
        // Events newer than `settled` stay unseen until they can be judged.
        if let Some(max) = events.iter().map(|e| e.ts).filter(|ts| *ts <= settled).max() {
            if max > self.last_seen_ts {
                self.last_seen_ts = max;
            }
        }
        if fresh.is_empty() || !self.rate_ok(now) {
            return None;
        }
        let n = fresh.len();
        let summary = "Privacy configuration drift detected".to_string();
        let body = if n == 1 {
            fresh[0].msg.clone()
        } else {
            format!("{n} privacy configuration drifts detected.")
        };
        self.last_fire = Some(now);
        Some(Notification { summary, body, critical: false })
    }

    fn advance_seen(&mut self, events: &[Event]) {
        if let Some(max) = events.iter().map(|e| e.ts).max() {
            if max > self.last_seen_ts {
                self.last_seen_ts = max;
            }
        }
    }
}

// ── sign-in pages (captive portals) ──────────────────────────────────────────

/// NetworkManager's NMConnectivityState values the tray cares about.
pub const CONNECTIVITY_PORTAL: u32 = 2;
pub const CONNECTIVITY_LIMITED: u32 = 3;
pub const CONNECTIVITY_FULL: u32 = 4;

/// What the tray does when NetworkManager's Connectivity changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalAction {
    Nothing,
    /// Ask whether to use the network's own DNS until the sign-in is done.
    Offer,
}

/// `new` is the Manager's Connectivity now, `dns` the status arkad reports,
/// `primary_is_vpn` whether the primary connection is a VPN, `activation` the
/// D-Bus path of the primary active connection and `offered_for` the activation
/// the tray last offered on.
///
/// PORTAL asks once per activation. Under strict encrypted DNS NetworkManager
/// often reports LIMITED first and PORTAL later, so nothing is keyed on the
/// previous value: LIMITED then PORTAL still asks, and a repeated PORTAL on the
/// same activation does not. It asks only while DNS is reported Encrypted, so
/// it never offers to pause what is already off or paused, and never over a VPN.
/// LIMITED and FULL ask nothing.
pub fn portal_action(new: u32, dns: &DnsStatus, primary_is_vpn: bool, activation: &str, offered_for: Option<&str>) -> PortalAction {
    if new == CONNECTIVITY_PORTAL
        && !primary_is_vpn
        && dns.is_encrypted()
        && offered_for != Some(activation)
    {
        PortalAction::Offer
    } else {
        PortalAction::Nothing
    }
}

/// A paused connection goes back to encrypted DNS when the paused device itself
/// has full access on IPv4 or IPv6 (its own Ip4Connectivity / Ip6Connectivity,
/// not the Manager's overall value: a working ethernet link must not restore
/// Wi-Fi that is still waiting for its sign-in).
pub fn device_is_full(ip4: u32, ip6: u32) -> bool {
    ip4 == CONNECTIVITY_FULL || ip6 == CONNECTIVITY_FULL
}

/// The notification that offers the pause. Not subject to NOTIFY_MIN_GAP_SECS.
#[derive(Clone, Debug, PartialEq)]
pub struct PortalOffer {
    pub summary: String,
    pub body: String,
    /// (key, label) pairs in the order the notification shows them.
    pub actions: Vec<(&'static str, &'static str)>,
}

pub const ACTION_PAUSE: &str = "pause";
pub const ACTION_DECLINE: &str = "no";

pub fn portal_offer(connection: &str) -> PortalOffer {
    PortalOffer {
        summary: "This network needs a sign-in page".to_string(),
        body: format!(
            "Use the network\u{2019}s own DNS on \u{201c}{}\u{201d} until you sign in? Until then, the network can see the names you look up.",
            shown_name(connection)
        ),
        actions: vec![(ACTION_PAUSE, "Use network DNS"), (ACTION_DECLINE, "Not now")],
    }
}

/// A connection name as shown in a notification. The name is chosen by the
/// network, so control characters are replaced and the length is bounded.
fn shown_name(name: &str) -> String {
    let mut out: String = name.chars().take(64).map(|c| if c.is_control() { '?' } else { c }).collect();
    if name.chars().count() > 64 { out.push_str("..."); }
    out
}

/// Arguments for `nmcli` that switch encrypted DNS off on the running
/// connection only. The saved profile is not touched, and the setting ends when
/// the link goes down. (resolvectl is not used: NetworkManager overwrites it.)
pub fn pause_args(iface: &str) -> Vec<String> {
    ["device", "modify", iface, "connection.dns-over-tls", "no"].map(String::from).to_vec()
}

/// Arguments for `nmcli` that re-apply the saved profile to the running connection.
pub fn reapply_args(iface: &str) -> Vec<String> {
    ["device", "reapply", iface].map(String::from).to_vec()
}

/// The sign-in check address to open, or None when there is nothing safe to open.
pub fn sign_in_uri(connectivity_check_uri: &str) -> Option<&str> {
    let uri = connectivity_check_uri.trim();
    (uri.starts_with("http://") || uri.starts_with("https://")).then_some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_requires_a_completed_recent_check() {
        for last in [None, Some(0), Some(101), Some(10)] {
            assert!(!check_is_fresh(last, 100, 30), "{last:?}");
        }
        assert!(check_is_fresh(Some(11), 100, 30));
        assert!(check_is_fresh(Some(100), 100, 30));
        assert!(!check_is_fresh(Some(100), 100, 0));
        assert!(!check_is_fresh(Some(88), 100, 4));
        assert!(check_is_fresh(Some(89), 100, 4));
    }

    #[test]
    fn failed_daemon_values_raise_attention() {
        for dns in ["Plaintext", "Degraded", "Error", "Initializing", "Unknown", "", "future-state"] {
            let mut e = enf_all_on();
            e.dns_ok = dns_active(dns);
            assert_eq!(level(&e, false), Level::Critical, "{dns}");
        }
        assert!(dns_active("Encrypted"));
        for (mode, sandbox) in [("None", "Active"), ("Ephemeral", "Inactive"), ("Ephemeral", "Partial"), ("Unknown", "Active"), ("Ephemeral", "Unknown")] {
            let mut e = enf_all_on();
            e.browser_ok = browser_active(mode, sandbox);
            assert_eq!(level(&e, false), Level::Critical, "{mode}/{sandbox}");
        }
        assert!(browser_active("Ephemeral", "Active"));
    }

    #[test]
    fn a_dns_exception_the_user_chose_is_shown_but_never_alarms() {
        let mut e = enf_all_on();
        e.dns_ok = dns_active("ForcedPlaintext");
        e.dns_user_off = dns_user_off("ForcedPlaintext");
        assert!(!e.dns_ok && e.dns_user_off);
        assert!(!e.all_active(), "never claims everything is in place");
        assert!(e.down().is_empty(), "a user choice isn't a failure");
        assert_eq!(level(&e, false), Level::Warn);
        let mut n = Notifier::new(60, 0);
        assert!(n.evaluate(&[], &e, 1000).is_none(), "no alarm for the user's own choice");
        assert!(e.rows().iter().any(|r| r.name == "Encrypted DNS" && r.detail.contains("your choice")));
    }

    #[test]
    fn alerts_never_promise_protection() {
        let mut e = enf_all_on();
        e.mac = false;
        let mut notifier = Notifier::new(60, 0);
        assert!(notifier.evaluate(&[], &e, 1000).is_none(), "not on the first check");
        let n = notifier.evaluate(&[], &e, 1030).unwrap();
        let text = format!("{} {}", n.summary, n.body).to_lowercase();
        assert!(!text.contains("protect"), "{text}");
        for l in [Level::Ok, Level::Warn, Level::Critical] {
            assert!(!l.header(0).to_lowercase().contains("protect"));
        }
    }

    fn enf_all_on() -> Enforcement {
        Enforcement {
            mac: true,
            dns_detail: "Quad9 (DoT)".into(),
            dns_ok: true,
            dns_user_off: false,
            dns_paused: false,
            hostname: true,
            ipv6: true,
            firewall: true,
            browser_detail: "Ephemeral sandbox".into(),
            browser_ok: true,
        }
    }

    #[test]
    fn unconfirmed_firewall_cannot_show_a_green_tray() {
        let mut e = enf_all_on();
        e.firewall = false;
        assert!(!e.all_active());
        assert_eq!(level(&e, false), Level::Critical);
        assert_eq!(e.down(), vec!["Firewall"]);
        assert!(!e.rows().last().unwrap().active);
    }

    #[test]
    fn all_active_and_rows() {
        let e = enf_all_on();
        assert!(e.all_active());
        assert_eq!(e.rows().len(), 6);
        assert!(e.rows().iter().all(|r| r.active));
        assert!(e.down().is_empty());
    }

    #[test]
    fn level_transitions() {
        let e = enf_all_on();
        assert_eq!(level(&e, false), Level::Ok);
        assert_eq!(level(&e, true), Level::Warn); // recent drift but all active now
        let mut d = e.clone();
        d.mac = false;
        assert_eq!(level(&d, false), Level::Critical); // down beats everything
        assert_eq!(level(&d, true), Level::Critical);
        assert_eq!(d.down(), vec!["Private network address"]);
    }

    #[test]
    fn icons_and_headers() {
        assert_eq!(Level::Ok.icon_name(), "security-high");
        assert_eq!(Level::Critical.icon_name(), "security-low");
        assert_eq!(Level::Ok.header(0), "Privacy settings are in place");
        assert_eq!(Level::Ok.header(2), "Privacy settings are in place, 2 put back today");
        assert_eq!(Level::Warn.header(1), "A privacy setting was put back, 1 today");
        assert_eq!(Level::Critical.header(0), "A privacy setting is off");
    }

    #[test]
    fn parse_arkad_jsonl() {
        let body = "\n{\"ts\":100,\"cat\":\"system\",\"ev\":\"ready\",\"msg\":\"all controls active\"}\n\
                    {\"ts\":200,\"cat\":\"system\",\"ev\":\"drift\",\"msg\":\"drift detected\"}\n\
                    garbage line\n\
                    {\"ts\":205,\"cat\":\"system\",\"ev\":\"recovered\",\"msg\":\"restored\"}\n";
        let evs = parse_log(body, 50);
        assert_eq!(evs.len(), 3); // blank + garbage skipped
        assert_eq!(evs[1].ev, "drift");
        assert_eq!(drifts_since(&evs, 0), 1);
        assert_eq!(drifts_since(&evs, 201), 0); // drift at 200 is before cutoff
    }

    #[test]
    fn parse_log_caps_to_recent() {
        let mut body = String::new();
        for i in 0..100 {
            body.push_str(&format!("{{\"ts\":{i},\"cat\":\"c\",\"ev\":\"x\",\"msg\":\"m\"}}\n"));
        }
        let evs = parse_log(&body, 10);
        assert_eq!(evs.len(), 10);
        assert_eq!(evs[0].ts, 90); // oldest kept is the 10th-from-last
        assert_eq!(evs[9].ts, 99);
    }

    #[test]
    fn notifier_positive_default_is_silent() {
        let mut n = Notifier::new(60, 0);
        let e = enf_all_on();
        // ready/recovered events, no drift → no notification ever
        let evs = vec![
            Event { ts: 10, cat: "system".into(), ev: "ready".into(), msg: "up".into() },
            Event { ts: 20, cat: "system".into(), ev: "recovered".into(), msg: "ok".into() },
        ];
        assert_eq!(n.evaluate(&evs, &e, 30), None);
    }

    #[test]
    fn notifier_fires_once_on_new_drift_then_rate_limits() {
        let mut n = Notifier::new(60, 0);
        let e = enf_all_on();
        let evs = vec![Event { ts: 100, cat: "system".into(), ev: "drift".into(), msg: "drifted".into() }];
        assert_eq!(n.evaluate(&evs, &e, 100), None, "too fresh to judge");
        let first = n.evaluate(&evs, &e, 130);
        assert!(first.is_some());
        assert!(!first.unwrap().critical);
        // same event again → already seen, no re-fire
        assert_eq!(n.evaluate(&evs, &e, 131), None);
        // a second drift arrives but within the rate-limit window → suppressed
        let evs2 = vec![
            Event { ts: 100, cat: "system".into(), ev: "drift".into(), msg: "drifted".into() },
            Event { ts: 140, cat: "system".into(), ev: "drift".into(), msg: "again".into() },
        ];
        assert_eq!(n.evaluate(&evs2, &e, 170), None); // 170-130 < 60
    }

    #[test]
    fn a_drift_fixed_at_once_never_pops_up() {
        // The Nokia case after sleep: ipv6 drift and recovery in the same second.
        let mut n = Notifier::new(60, 0);
        let e = enf_all_on();
        let evs = vec![
            Event { ts: 100, cat: "ipv6".into(), ev: "drift".into(), msg: "could not be confirmed".into() },
            Event { ts: 100, cat: "ipv6".into(), ev: "recovered".into(), msg: "confirmed again".into() },
        ];
        for t in [100, 130, 160, 400] {
            assert_eq!(n.evaluate(&evs, &e, t), None, "t={t}");
        }
        // A drift in another setting that stays unfixed still warns.
        let mut evs2 = evs.clone();
        evs2.push(Event { ts: 500, cat: "dns".into(), ev: "drift".into(), msg: "dns drifted".into() });
        assert_eq!(n.evaluate(&evs2, &e, 510), None);
        assert_eq!(n.evaluate(&evs2, &e, 530).unwrap().body, "dns drifted");
    }

    #[test]
    fn notifier_critical_on_down_edge_only() {
        let mut n = Notifier::new(60, 0);
        let mut e = enf_all_on();
        e.dns_ok = false;
        e.dns_detail = "Unknown".into();
        let evs: Vec<Event> = vec![];
        assert_eq!(n.evaluate(&evs, &e, 500), None, "first check: wait for the next one");
        let second = n.evaluate(&evs, &e, 530);
        assert!(second.as_ref().unwrap().critical);
        assert!(second.unwrap().body.contains("DNS"));
        // still down next tick → no repeat (once per episode)
        assert_eq!(n.evaluate(&evs, &e, 560), None);
    }

    #[test]
    fn a_setting_that_heals_by_the_next_check_never_alarms() {
        // The Nokia case: firewalld slow right after boot, fine 30 s later.
        let mut n = Notifier::new(60, 0);
        let mut down = enf_all_on();
        down.firewall = false;
        assert_eq!(n.evaluate(&[], &down, 100), None);
        assert_eq!(n.evaluate(&[], &enf_all_on(), 130), None);
        // A later, lasting failure still alarms.
        assert_eq!(n.evaluate(&[], &down, 700), None);
        assert!(n.evaluate(&[], &down, 730).unwrap().body.contains("Firewall"));
    }

    #[test]
    fn a_paused_dns_is_shown_amber_and_never_alarms() {
        let mut e = enf_all_on();
        e.dns_ok = dns_active("PausedForSignIn");
        e.dns_user_off = dns_user_off("PausedForSignIn");
        e.dns_paused = dns_paused("PausedForSignIn");
        assert!(!e.dns_ok && !e.dns_user_off && e.dns_paused);
        assert!(!e.all_active(), "never claims everything is in place");
        assert!(e.down().is_empty(), "a pause the user chose is not a failure");
        assert_eq!(level(&e, false), Level::Warn);
        let mut n = Notifier::new(60, 0);
        assert!(n.evaluate(&[], &e, 1000).is_none());
        assert!(n.evaluate(&[], &e, 1100).is_none());
        assert!(e.rows().iter().any(|r| r.name == "Encrypted DNS" && r.detail.contains("sign-in")));
        assert!(!dns_paused("ForcedPlaintext") && !dns_paused("Encrypted"));
    }

    #[test]
    fn connectivity_changes_decide_the_portal_action() {
        use DnsStatus::*;
        use PortalAction::*;
        let (p, l, f) = (CONNECTIVITY_PORTAL, CONNECTIVITY_LIMITED, CONNECTIVITY_FULL);
        let act = "/org/freedesktop/NetworkManager/ActiveConnection/7";
        let other = "/org/freedesktop/NetworkManager/ActiveConnection/8";
        // (new, dns, primary is a VPN, activation last offered on, expected)
        let table: Vec<(u32, DnsStatus, bool, Option<&str>, PortalAction)> = vec![
            (p, Encrypted, false, None, Offer),
            // LIMITED came first (nothing offered), then PORTAL on the same activation: asks.
            (l, Encrypted, false, None, Nothing),
            (p, Encrypted, false, None, Offer),
            // The same activation reports PORTAL again: no second offer.
            (p, Encrypted, false, Some(act), Nothing),
            // A new activation (another network, or the same one reconnected) asks again.
            (p, Encrypted, false, Some(other), Offer),
            (p, ForcedPlaintext, false, None, Nothing),
            (p, PausedForSignIn, false, None, Nothing),
            (p, Encrypted, true, None, Nothing),
            (p, Degraded, false, None, Nothing),
            (p, Unknown("Initializing".into()), false, None, Nothing),
            (l, PausedForSignIn, false, None, Nothing),
            (f, Encrypted, false, None, Nothing),
            (f, PausedForSignIn, false, None, Nothing),
            (1, Encrypted, false, None, Nothing),
            (0, Encrypted, false, None, Nothing),
        ];
        for (new, dns, vpn, offered, want) in table {
            assert_eq!(portal_action(new, &dns, vpn, act, offered), want, "{new}, {dns:?}, vpn={vpn}, offered={offered:?}");
        }
    }

    #[test]
    fn a_paused_device_is_restored_only_by_its_own_full_access() {
        let (p, l, f) = (CONNECTIVITY_PORTAL, CONNECTIVITY_LIMITED, CONNECTIVITY_FULL);
        assert!(device_is_full(f, 0));
        assert!(device_is_full(0, f));
        assert!(device_is_full(f, p));
        assert!(!device_is_full(p, l));
        assert!(!device_is_full(p, 0));
        assert!(!device_is_full(l, l));
        assert!(!device_is_full(1, 1));
    }

    #[test]
    fn the_portal_offer_says_what_the_network_can_see_and_never_promises_protection() {
        let o = portal_offer("Cafe Free WiFi");
        assert_eq!(o.summary, "This network needs a sign-in page");
        assert_eq!(o.body, "Use the network\u{2019}s own DNS on \u{201c}Cafe Free WiFi\u{201d} until you sign in? Until then, the network can see the names you look up.");
        assert_eq!(o.actions, vec![("pause", "Use network DNS"), ("no", "Not now")]);
        let text = format!("{} {}", o.summary, o.body).to_lowercase();
        for banned in ["secure", "safe", "private", "protect", "anonymous", "100%"] {
            assert!(!text.contains(banned), "{banned}");
        }
        assert!(portal_offer("a\nb").body.contains("a?b"), "a name cannot add lines");
    }

    #[test]
    fn pausing_changes_the_running_connection_only_and_restoring_reapplies() {
        assert_eq!(pause_args("wlp2s0"), ["device", "modify", "wlp2s0", "connection.dns-over-tls", "no"]);
        assert_eq!(reapply_args("wlp2s0"), ["device", "reapply", "wlp2s0"]);
        assert!(!pause_args("wlp2s0").iter().any(|a| a == "connection" || a == "resolvectl"), "no saved-profile edit");
    }

    #[test]
    fn nothing_is_opened_without_a_web_address() {
        assert_eq!(sign_in_uri("http://fedoraproject.org/static/hotspot.txt"), Some("http://fedoraproject.org/static/hotspot.txt"));
        assert_eq!(sign_in_uri(" https://example.org/x \n"), Some("https://example.org/x"));
        for bad in ["", "   ", "file:///etc/passwd", "ftp://x", "--help", "javascript:alert(1)"] {
            assert_eq!(sign_in_uri(bad), None, "{bad:?}");
        }
    }
}
