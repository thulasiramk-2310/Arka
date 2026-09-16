//! Pure, binding-agnostic core for the Privacy Activity tray.
//!
//! No D-Bus and no GUI live here — only the honest state/event model and the
//! rules that decide (a) the tray icon level, (b) the panel rows, (c) how many
//! drifts happened today, and (d) whether a new event is worth a notification.
//! Keeping this pure makes the trust-critical behaviour unit-testable without a
//! running Plasma session.
//!
//! Honesty rules baked in (see CLAUDE.md "Standing rules"):
//!   * We report enforcement STATE and NOTABLE EVENTS — never per-connection or
//!     per-domain traffic (that would be a surveillance layer; phase 2 only).
//!   * The positive default is silence: "Privacy intact — 0 drifts today".
//!   * Only a real, new drift (warn) or a protection currently DOWN (critical)
//!     may notify. Routine/info events never do. Notifications are rate-limited
//!     and coalesced so warning-fatigue can't set in.

use serde::Deserialize;

/// Live snapshot of arkad's enforcement, read from its D-Bus properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Enforcement {
    pub mac: bool,
    pub dns_detail: String,
    pub dns_ok: bool,
    pub hostname: bool,
    pub ipv6: bool,
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
    /// The five protection rows, in a stable order.
    pub fn rows(&self) -> Vec<Row> {
        vec![
            Row { name: "Hardware address", detail: if self.mac { "Randomized per network".into() } else { "Not randomized".into() }, active: self.mac },
            Row { name: "DNS", detail: self.dns_detail.clone(), active: self.dns_ok },
            Row { name: "Hostname", detail: if self.hostname { "Masked to \u{201c}arka\u{201d}".into() } else { "Not masked".into() }, active: self.hostname },
            Row { name: "IPv6", detail: if self.ipv6 { "Temporary addresses on".into() } else { "Temporary addresses off".into() }, active: self.ipv6 },
            Row { name: "Browser", detail: self.browser_detail.clone(), active: self.browser_ok },
        ]
    }

    /// True only when every protection is currently active.
    pub fn all_active(&self) -> bool {
        self.mac && self.dns_ok && self.hostname && self.ipv6 && self.browser_ok
    }

    /// Names of protections that are currently NOT active (for a critical alert).
    pub fn down(&self) -> Vec<&'static str> {
        self.rows().into_iter().filter(|r| !r.active).map(|r| r.name).collect()
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
            Level::Ok => format!("Privacy intact — {drifts_today} drift{} today", plural(drifts_today)),
            Level::Warn => format!("Privacy self-healed — {drifts_today} drift{} today", plural(drifts_today)),
            Level::Critical => "Attention — a protection is not active".to_string(),
        }
    }
}

fn plural(n: usize) -> &'static str { if n == 1 { "" } else { "s" } }

/// Decide the tray level from live enforcement plus whether a drift happened
/// recently. A protection currently down always wins (critical).
pub fn level(enf: &Enforcement, recent_drift: bool) -> Level {
    if !enf.all_active() {
        Level::Critical
    } else if recent_drift {
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
    /// remembers the previous "something is down" state so critical fires on
    /// the transition into down, not every tick.
    was_down: bool,
}

impl Notifier {
    pub fn new(min_gap_secs: u64, start_ts: u64) -> Self {
        Self { min_gap_secs, last_fire: None, last_seen_ts: start_ts, was_down: false }
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
        // Critical: a protection is currently down. Fire only on the transition
        // into "down" (edge), and respect the rate limit.
        let down = enf.down();
        let is_down = !down.is_empty();
        let critical = if is_down && !self.was_down && self.rate_ok(now) {
            Some(Notification {
                summary: "Privacy protection not active".to_string(),
                body: format!("Not active: {}. arkad will try to re-apply it.", down.join(", ")),
                critical: true,
            })
        } else {
            None
        };
        self.was_down = is_down;
        if let Some(n) = critical {
            self.last_fire = Some(now);
            // still advance last_seen so drift events up to now aren't re-counted
            self.advance_seen(events);
            return Some(n);
        }

        // Warn: new drift events since we last looked.
        let fresh: Vec<&Event> = events.iter().filter(|e| e.is_drift() && e.ts > self.last_seen_ts).collect();
        self.advance_seen(events);
        if fresh.is_empty() || !self.rate_ok(now) {
            return None;
        }
        let n = fresh.len();
        let summary = "Privacy drift auto-corrected".to_string();
        let body = if n == 1 {
            fresh[0].msg.clone()
        } else {
            format!("{n} privacy drifts detected and re-enforced.")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn enf_all_on() -> Enforcement {
        Enforcement {
            mac: true,
            dns_detail: "Quad9 (DoT)".into(),
            dns_ok: true,
            hostname: true,
            ipv6: true,
            browser_detail: "Ephemeral sandbox".into(),
            browser_ok: true,
        }
    }

    #[test]
    fn all_active_and_rows() {
        let e = enf_all_on();
        assert!(e.all_active());
        assert_eq!(e.rows().len(), 5);
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
        assert_eq!(d.down(), vec!["Hardware address"]);
    }

    #[test]
    fn icons_and_headers() {
        assert_eq!(Level::Ok.icon_name(), "security-high");
        assert_eq!(Level::Critical.icon_name(), "security-low");
        assert_eq!(Level::Ok.header(0), "Privacy intact — 0 drifts today");
        assert_eq!(Level::Ok.header(1), "Privacy intact — 1 drift today");
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
        let first = n.evaluate(&evs, &e, 100);
        assert!(first.is_some());
        assert!(!first.unwrap().critical);
        // same event again → already seen, no re-fire
        assert_eq!(n.evaluate(&evs, &e, 101), None);
        // a second drift arrives but within the rate-limit window → suppressed
        let evs2 = vec![
            Event { ts: 100, cat: "system".into(), ev: "drift".into(), msg: "drifted".into() },
            Event { ts: 130, cat: "system".into(), ev: "drift".into(), msg: "again".into() },
        ];
        assert_eq!(n.evaluate(&evs2, &e, 130), None); // 130-100 < 60
    }

    #[test]
    fn notifier_critical_on_down_edge_only() {
        let mut n = Notifier::new(60, 0);
        let mut e = enf_all_on();
        e.dns_ok = false;
        e.dns_detail = "Unknown".into();
        let evs: Vec<Event> = vec![];
        let first = n.evaluate(&evs, &e, 500);
        assert!(first.as_ref().unwrap().critical);
        assert!(first.unwrap().body.contains("DNS"));
        // still down next tick → no repeat (edge-triggered)
        assert_eq!(n.evaluate(&evs, &e, 501), None);
    }
}
