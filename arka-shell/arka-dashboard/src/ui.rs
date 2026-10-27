use std::sync::mpsc::Sender;

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
use arka_update::status::UpdateStatus;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::dbus::{call_enforce_all, fetch_updates};
use crate::state::{DashboardState, StateUpdate};

const CSS: &str = include_str!("style.css");

/// arkad re-checks every 60 s; older than three rounds counts as stale, and a
/// stale reading is never shown as green.
const STALE_SECS: u64 = 180;

// --- helpers ----------------------------------------------------------------

fn load_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn set_badge(label: &gtk4::Label, text: &str, good: bool, warn: bool) {
    label.set_text(text);
    label.remove_css_class("badge-green");
    label.remove_css_class("badge-yellow");
    label.remove_css_class("badge-red");
    if good        { label.add_css_class("badge-green"); }
    else if warn   { label.add_css_class("badge-yellow"); }
    else           { label.add_css_class("badge-red"); }
}

fn make_icon(name: &str) -> gtk4::Image {
    let img = gtk4::Image::from_icon_name(name);
    img.set_icon_size(gtk4::IconSize::Normal);
    img
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.add_css_class(class);
    l.set_xalign(0.0);
    l.set_halign(gtk4::Align::Start);
    l.set_wrap(true);
    l
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn ago(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s ago"),
        60..=3599 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

/// A page: serif title, subtitle, then `body`, centred and scrollable.
fn page(title: &str, sub: &str, body: &impl IsA<gtk4::Widget>) -> gtk4::ScrolledWindow {
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    col.set_margin_top(32);
    col.set_margin_bottom(32);
    col.set_margin_start(24);
    col.set_margin_end(24);
    col.append(&label(title, "dash-title"));
    let s = label(sub, "dash-sub");
    s.set_margin_top(8);
    col.append(&s);
    body.as_ref().set_margin_top(24);
    col.append(body);
    let clamp = adw::Clamp::builder().maximum_size(680).tightening_threshold(560).child(&col).build();
    gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&clamp)
        .build()
}

// --- privacy rows -----------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Row { Checking, Configured, Attention, Unavailable, Stale, OffHere }

/// One setting: what it does (never a promise of protection) + its status.
struct SettingRow { dot: gtk4::Box, state: gtk4::Label, act: gtk4::Label }

impl SettingRow {
    fn new(parent: &gtk4::Box, title: &str, does: &str) -> Self {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        row.add_css_class("dash-row");
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("dot");
        dot.set_valign(gtk4::Align::Start);
        dot.set_margin_top(7);
        let col = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        col.set_hexpand(true);
        col.append(&label(title, "dash-row-title"));
        col.append(&label(does, "dash-row-sub"));
        let act = label("", "dash-row-act");
        act.set_visible(false);
        col.append(&act);
        let state = gtk4::Label::new(Some("Checking…"));
        state.add_css_class("state");
        state.set_valign(gtk4::Align::Start);
        row.append(&dot);
        row.append(&col);
        row.append(&state);
        parent.append(&row);
        SettingRow { dot, state, act }
    }

    fn set(&self, st: Row) {
        for c in ["ok", "warn"] {
            self.dot.remove_css_class(c);
            self.state.remove_css_class(c);
        }
        let (text, class) = match st {
            Row::Checking => ("Checking…", None),
            Row::Configured => ("Configured", Some("ok")),
            Row::Attention => ("Needs attention", Some("warn")),
            Row::Unavailable => ("Unavailable", None),
            Row::Stale => ("Not recently checked", None),
            Row::OffHere => ("Off on this network", Some("warn")),
        };
        self.state.set_text(text);
        if let Some(c) = class {
            self.dot.add_css_class(c);
            self.state.add_css_class(c);
        }
        self.act.set_visible(matches!(st, Row::Attention | Row::OffHere));
        self.act.set_text(if st == Row::OffHere {
            "You allowed unencrypted DNS on the network you're connected to. Other networks still require encryption."
        } else {
            "Try Re-apply settings below. If it stays this way, the Activity page shows what arkad saw."
        });
    }
}

// --- system health (arka-pulse) ---------------------------------------------

/// Handles for the live-updated widgets in the System Health group.
struct HealthRows {
    verdict: adw::ActionRow,
    verdict_badge: gtk4::Label,
    cpu: gtk4::Label,
    mem: gtk4::Label,
    temp: gtk4::Label,
    pred: gtk4::Label,
}

fn value_row(title: &str, icon: &str) -> (adw::ActionRow, gtk4::Label) {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_activatable(false);
    row.add_prefix(&make_icon(icon));
    let val = gtk4::Label::new(Some("…"));
    val.set_valign(gtk4::Align::Center);
    val.add_css_class("dim-label");
    row.add_suffix(&val);
    (row, val)
}

/// The "one surface, two depths" reliability view from
/// docs/RELIABILITY-ARKA-PULSE.md: a plain verdict on top, the evidence
/// (CPU/memory/temperature + any predicted instability) below it.
fn build_health_group() -> (adw::PreferencesGroup, HealthRows) {
    let group = adw::PreferencesGroup::new();
    group.set_title("System Health");
    group.set_description(Some("Your machine's reliability — watched locally, shared with no one"));

    let verdict = adw::ActionRow::new();
    verdict.set_title("Checking system health…");
    verdict.add_prefix(&make_icon("emblem-ok-symbolic"));
    verdict.set_activatable(false);
    let verdict_badge = gtk4::Label::new(Some("…"));
    verdict_badge.set_valign(gtk4::Align::Center);
    verdict_badge.add_css_class("badge-yellow");
    verdict.add_suffix(&verdict_badge);
    group.add(&verdict);

    let (cpu_row, cpu)   = value_row("Processor", "computer-symbolic");
    let (mem_row, mem)   = value_row("Memory", "drive-harddisk-symbolic");
    let (temp_row, temp) = value_row("Temperature", "dialog-information-symbolic");
    let (pred_row, pred) = value_row("Predicted instability", "dialog-warning-symbolic");
    group.add(&cpu_row);
    group.add(&mem_row);
    group.add(&temp_row);
    group.add(&pred_row);

    (group, HealthRows { verdict, verdict_badge, cpu, mem, temp, pred })
}

fn fmt_pct(v: f64) -> String {
    if v < 0.0 { "—".into() } else { format!("{v:.0}%") }
}

fn fmt_temp(v: f64) -> String {
    if v < 0.0 { "—".into() } else { format!("{v:.0}°C") }
}

fn apply_health(r: &crate::state::ReliabilityState, rows: &HealthRows) {
    if !r.available {
        rows.verdict.set_title("System health unavailable");
        rows.verdict.set_subtitle("arka-pulse isn’t running on this system");
        set_badge(&rows.verdict_badge, "Offline", false, true);
        rows.cpu.set_text("—");
        rows.mem.set_text("—");
        rows.temp.set_text("—");
        rows.pred.set_text("—");
        return;
    }

    // health/summary are the deterministic DETECT verdict — safe to state plainly.
    let (title, badge, good, warn) = match r.health.as_str() {
        "OK"   => ("Your computer is healthy", "Healthy", true, false),
        "WARN" => ("Attention may be needed", "Warning", false, true),
        "CRIT" => ("Critical — action recommended", "Critical", false, false),
        _      => ("Checking system health…", "…", false, true),
    };
    rows.verdict.set_title(title);
    rows.verdict.set_subtitle(&glib::markup_escape_text(&r.summary));
    set_badge(&rows.verdict_badge, badge, good, warn);

    rows.cpu.set_text(&fmt_pct(r.cpu_util));
    rows.mem.set_text(&fmt_pct(r.mem_pct));
    rows.temp.set_text(&fmt_temp(r.temp_max));

    // Predictions are a HEURISTIC — always framed as *possible*, never certain.
    match &r.prediction {
        Some((summary, prob)) => {
            rows.pred.set_text(&format!("Estimated: possible {summary} (~{:.0}%)", prob * 100.0))
        }
        None => rows.pred.set_text("None estimated"),
    }
}

// --- timeline ---------------------------------------------------------------

struct LogEvent {
    ts:  u64,
    cat: String,
    ev:  String,
    msg: String,
}

fn extract_u64(s: &str, key: &str) -> Option<u64> {
    let prefix = format!("\"{}\":", key);
    let start = s.find(&prefix)? + prefix.len();
    let rest = &s[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn extract_str(s: &str, key: &str) -> Option<String> {
    let prefix = format!("\"{}\":\"", key);
    let start = s.find(&prefix)? + prefix.len();
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_log_line(line: &str) -> Option<LogEvent> {
    Some(LogEvent {
        ts:  extract_u64(line, "ts")?,
        cat: extract_str(line, "cat")?,
        ev:  extract_str(line, "ev")?,
        msg: extract_str(line, "msg")?,
    })
}

fn event_icon(cat: &str, ev: &str) -> &'static str {
    if ev == "drift" { return "dialog-warning-symbolic"; }
    match cat {
        "dns"      => "network-server-symbolic",
        "mac"      => "network-wireless-symbolic",
        "hostname" => "computer-symbolic",
        "ipv6"     => "preferences-system-network-symbolic",
        "sandbox"  => "security-high-symbolic",
        "system"   => "dialog-information-symbolic",
        _          => "emblem-system-symbolic",
    }
}

fn event_class(ev: &str) -> &'static str {
    match ev {
        "drift"     => "ev-drift",
        "recovered" => "ev-recovered",
        "started" | "ready" | "active" => "ev-info",
        _           => "ev-other",
    }
}

/// Count recorded events. The log does not measure DNS coverage or browser launches.
fn weekly_report_card(all: &[LogEvent]) -> adw::PreferencesGroup {
    let now = glib::DateTime::now_local()
        .map(|d| d.to_unix() as u64)
        .unwrap_or(0);
    let week_ago = now.saturating_sub(7 * 24 * 3600);
    let week: Vec<&LogEvent> = all.iter().filter(|e| e.ts >= week_ago && e.ts <= now).collect();

    let drifts    = week.iter().filter(|e| e.ev == "drift").count();
    let recovered = week.iter().filter(|e| e.ev == "recovered").count();

    let group = adw::PreferencesGroup::new();
    group.set_title("This Week");
    group.set_description(Some(if week.is_empty() {
        "No event records available for the last 7 days"
    } else {
        "Recorded events · last 7 days"
    }));

    let stats: [(&str, String, &str); 3] = [
        ("Privacy events", week.len().to_string(), "view-list-symbolic"),
        (
            "Drift events",
            drifts.to_string(),
            "security-high-symbolic",
        ),
        (
            "Recovery events",
            recovered.to_string(),
            "view-refresh-symbolic",
        ),
    ];

    for (title, value, icon_name) in stats {
        let row = adw::ActionRow::new();
        row.set_title(title);
        let icon = gtk4::Image::from_icon_name(icon_name);
        row.add_prefix(&icon);
        let val = gtk4::Label::new(Some(&value));
        val.add_css_class("accent");
        row.add_suffix(&val);
        group.add(&row);
    }
    group
}

fn populate_timeline(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let today = glib::DateTime::now_local().map(|d| d.format("%Y-%m-%d").unwrap_or_default());
    let today_str = today.unwrap_or_default();

    let content = std::fs::read_to_string("/var/log/arkaos/privacy.jsonl").unwrap_or_default();
    let all_events: Vec<LogEvent> = content.lines().filter_map(parse_log_line).collect();

    let report = weekly_report_card(&all_events);
    report.set_margin_bottom(16);
    container.append(&report);

    let mut events: Vec<LogEvent> = all_events
        .into_iter()
        .filter(|e| {
            glib::DateTime::from_unix_local(e.ts as i64)
                .map(|d| d.format("%Y-%m-%d").unwrap_or_default() == today_str)
                .unwrap_or(false)
        })
        .collect();

    events.reverse(); // newest first

    if events.is_empty() {
        let empty = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        empty.set_halign(gtk4::Align::Center);
        empty.set_valign(gtk4::Align::Center);
        empty.set_margin_top(60);

        let icon = gtk4::Image::from_icon_name("security-high-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("dim-label");

        let lbl = gtk4::Label::new(Some("No events today yet"));
        lbl.add_css_class("dim-label");

        let sub = gtk4::Label::new(Some("arkad logs privacy events here as they happen"));
        sub.add_css_class("dim-label");
        sub.add_css_class("caption");

        empty.append(&icon);
        empty.append(&lbl);
        empty.append(&sub);
        container.append(&empty);
        return;
    }

    // Group events: use a single PreferencesGroup for today
    let group = adw::PreferencesGroup::new();
    group.set_title(&format!("Today — {} events", events.len()));

    for ev in &events {
        let time_str = glib::DateTime::from_unix_local(ev.ts as i64)
            .and_then(|d| d.format("%H:%M").map(|s| s.to_string()))
            .unwrap_or_else(|_| "??:??".to_string());

        let row = adw::ActionRow::new();
        row.set_title(&glib::markup_escape_text(&ev.msg));
        row.set_subtitle(&glib::markup_escape_text(&format!("{time_str}  ·  {}", ev.cat)));

        let icon = gtk4::Image::from_icon_name(event_icon(&ev.cat, &ev.ev));
        icon.set_pixel_size(16);

        icon.add_css_class(event_class(&ev.ev));

        row.add_prefix(&icon);
        group.add(&row);
    }

    container.append(&group);
}

// --- build ------------------------------------------------------------------

pub fn build(
    window: &adw::ApplicationWindow,
    tx: Sender<StateUpdate>,
) -> Box<dyn Fn(StateUpdate)> {
    load_css();

    // ── Overview: what ArkaOS has set up, as last checked ────────────────────
    let rows_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rows_box.add_css_class("dash-rows");
    let dns = SettingRow::new(&rows_box, "Encrypted DNS", "Requires system DNS lookups to be encrypted.");
    let mac = SettingRow::new(&rows_box, "Private network address", "Uses a random hardware address on network connections.");
    let host = SettingRow::new(&rows_box, "Computer name", "Keeps the name chosen in setup: \"arka\" unless you picked your own.");
    let ipv6 = SettingRow::new(&rows_box, "Temporary IPv6 addresses", "Uses temporary IPv6 addresses that change over time.");
    let sandbox = SettingRow::new(&rows_box, "Sandboxed browser", "Opens Firefox in a sandbox that sees only your Downloads folder.");
    let fw = SettingRow::new(&rows_box, "Firewall", "Rejects connections other computers try to open to this one.");

    // Per-network exception for networks that block encrypted DNS (hotels,
    // captive portals). Shown only when DNS isn't encrypted right now.
    let dns_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    dns_box.add_css_class("dash-dns");
    dns_box.set_margin_top(16);
    dns_box.set_visible(false);
    let dns_note = label("", "dash-note");
    let dns_btn = gtk4::Button::new();
    dns_btn.add_css_class("pill");
    dns_btn.set_halign(gtk4::Align::Start);
    dns_box.append(&dns_note);
    dns_box.append(&dns_btn);
    rows_box.append(&dns_box);
    // (allow?, connection name) for the button, set by update_dns_box.
    let dns_target: DnsTarget = std::rc::Rc::new(std::cell::RefCell::new(None));
    {
        let (note, btn, target) = (dns_note.clone(), dns_btn.clone(), dns_target.clone());
        dns_btn.connect_clicked(move |b| {
            let Some((allow, conn)) = target.borrow().clone() else { return };
            b.set_sensitive(false);
            let (note, btn) = (note.clone(), btn.clone());
            glib::spawn_future_local(async move {
                let result = gtk4::gio::spawn_blocking(move || set_dns_exception(&conn, allow)).await
                    .unwrap_or_else(|_| Err("Could not confirm whether the DNS setting changed. Recheck before retrying.".into()));
                btn.set_sensitive(true);
                match result {
                    Ok(()) => note.set_text("Setting saved. Waiting for the privacy service to check it."),
                    Err(message) => note.set_text(&message),
                }
            });
        });
    }

    let fresh = label("Checking…", "dash-note");
    fresh.set_margin_top(14);

    let reapply = gtk4::Button::with_label("Re-apply settings");
    reapply.add_css_class("pill");
    reapply.set_halign(gtk4::Align::Start);
    let reapply_note = label("Asks for an administrator password.", "dash-note");
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    actions.set_margin_top(20);
    reapply_note.set_valign(gtk4::Align::Center);
    reapply_note.set_wrap(false);
    actions.append(&reapply);
    actions.append(&reapply_note);

    let tech = label("", "mono");
    tech.set_selectable(true);
    let expander = gtk4::Expander::new(Some("Technical details"));
    expander.set_child(Some(&tech));
    expander.set_margin_top(24);

    let overview_body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    overview_body.append(&rows_box);
    overview_body.append(&fresh);
    overview_body.append(&actions);
    overview_body.append(&expander);
    let overview = page("Privacy", "What ArkaOS has set up on this computer. Covers only what's listed here.", &overview_body);

    // ── Activity ─────────────────────────────────────────────────────────────
    let timeline_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    populate_timeline(&timeline_content);
    let activity = page("Activity", "What arkad recorded on this computer. Nothing here leaves it.", &timeline_content);

    // ── Health (arka-pulse: read-only, estimates labelled as such) ───────────
    let (health_group, health_rows) = build_health_group();
    let health = page("Health", "Watched locally by arka-pulse. Predictions are estimates, never certainties.", &health_group);

    // ── Updates & recovery ───────────────────────────────────────────────────
    let upd_rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    upd_rows.add_css_class("dash-rows");
    let mut upd_vals = Vec::new();
    for t in ["Installed version", "Downloaded update", "Restart needed", "Previous version"] {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        row.add_css_class("dash-row");
        let k = label(t, "dash-row-title");
        k.set_hexpand(true);
        let v = gtk4::Label::new(Some("Checking…"));
        v.add_css_class("state");
        v.set_wrap(true);
        v.set_xalign(1.0);
        row.append(&k);
        row.append(&v);
        upd_rows.append(&row);
        upd_vals.push(v);
    }
    let upd_note = label(
        "Switching back to the previous version changes the system only. Files and settings \
         in your home folder stay as they are now; older copies of personal files are not restored.",
        "dash-note",
    );
    upd_note.set_margin_top(16);
    let upd_err = label("", "dash-note");
    upd_err.set_visible(false);
    upd_err.set_margin_top(8);
    let open_updates = gtk4::Button::with_label("Open Updates");
    open_updates.add_css_class("pill");
    open_updates.set_halign(gtk4::Align::Start);
    open_updates.set_margin_top(20);
    open_updates.connect_clicked(|_| { let _ = std::process::Command::new("arka-update").spawn(); });
    let upd_body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    upd_body.append(&upd_rows);
    upd_body.append(&upd_note);
    upd_body.append(&upd_err);
    upd_body.append(&open_updates);
    let updates = page("Updates & recovery", "Which version of ArkaOS is running, and what you can go back to.", &upd_body);

    // ── navigation: sidebar + content, with Plasma's real window controls ───
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    let pages: [(&str, &str, &str, &gtk4::ScrolledWindow); 4] = [
        ("overview", "Overview", "security-high-symbolic", &overview),
        ("activity", "Activity", "document-open-recent-symbolic", &activity),
        ("health", "Health", "computer-symbolic", &health),
        ("updates", "Updates & recovery", "system-software-update-symbolic", &updates),
    ];
    let list = gtk4::ListBox::new();
    list.add_css_class("navigation-sidebar");
    for (id, title, icon, w) in pages {
        stack.add_named(w, Some(id));
        let r = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        r.set_margin_top(6);
        r.set_margin_bottom(6);
        r.append(&make_icon(icon));
        r.append(&gtk4::Label::new(Some(title)));
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&r));
        row.set_widget_name(id);
        list.append(&row);
    }
    {
        let (stack, tc, tx) = (stack.clone(), timeline_content.clone(), tx.clone());
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let id = row.widget_name();
            stack.set_visible_child_name(&id);
            match id.as_str() {
                "activity" => populate_timeline(&tc),
                "updates" => fetch_updates(tx.clone()),
                _ => {}
            }
        });
    }
    list.select_row(list.row_at_index(0).as_ref());

    let side_tb = adw::ToolbarView::new();
    let side_hb = adw::HeaderBar::new();
    side_hb.set_show_end_title_buttons(false);
    side_hb.set_title_widget(Some(&adw::WindowTitle::new("ArkaOS", "")));
    side_tb.add_top_bar(&side_hb);
    side_tb.set_content(Some(&list));
    let side_page = adw::NavigationPage::new(&side_tb, "ArkaOS");

    let content_tb = adw::ToolbarView::new();
    let content_hb = adw::HeaderBar::new();
    content_hb.set_title_widget(Some(&adw::WindowTitle::new("Privacy Dashboard", "")));
    content_tb.add_top_bar(&content_hb);
    content_tb.set_content(Some(&stack));
    let content_page = adw::NavigationPage::new(&content_tb, "Privacy Dashboard");

    let split = adw::NavigationSplitView::new();
    split.set_sidebar(Some(&side_page));
    split.set_content(Some(&content_page));
    split.set_min_sidebar_width(200.0);
    split.set_max_sidebar_width(240.0);
    {
        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 600sp").expect("breakpoint"));
        bp.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(bp);
    }

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&split));
    window.set_content(Some(&toast_overlay));

    // Keyboard: Ctrl+1..4 switch pages.
    {
        let keys = gtk4::EventControllerKey::new();
        let list = list.clone();
        keys.connect_key_pressed(move |_, key, _, m| {
            use gtk4::gdk::Key;
            if m.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
                let i = match key { Key::_1 => 0, Key::_2 => 1, Key::_3 => 2, Key::_4 => 3, _ => return glib::Propagation::Proceed };
                list.select_row(list.row_at_index(i).as_ref());
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(keys);
    }

    {
        let (tx, reapply2) = (tx.clone(), reapply.clone());
        reapply.connect_clicked(move |_| {
            reapply2.set_sensitive(false);
            call_enforce_all(tx.clone());
        });
    }

    // ── updater ──────────────────────────────────────────────────────────────
    Box::new(move |update: StateUpdate| match update {
        StateUpdate::Full(s) => {
            apply_privacy(&s, [&dns, &mac, &host, &ipv6, &sandbox, &fw], &fresh, &tech);
            update_dns_box(&s, &dns_box, &dns_note, &dns_btn, &dns_target);
            apply_health(&s.reliability, &health_rows);
        }
        StateUpdate::EnforceResult(r) => {
            reapply.set_sensitive(true);
            let msg = match r {
                Ok(()) => "Settings re-applied. Statuses update after the next check.".to_string(),
                Err(e) => format!("Couldn't re-apply settings: {e}"),
            };
            toast_overlay.add_toast(adw::Toast::new(&msg));
        }
        StateUpdate::Updates(r) => apply_updates(r, &upd_vals, &upd_err),
    })
}

fn apply_privacy(s: &DashboardState, rows: [&SettingRow; 6], fresh: &gtk4::Label, tech: &gtk4::Label) {
    if !s.available {
        for r in rows { r.set(Row::Unavailable); }
        fresh.set_text("The ArkaOS privacy service isn't answering. Statuses can't be shown until it does.");
        tech.set_text("Service: org.arka.arkad on the system D-Bus — not reachable");
        return;
    }
    let now = now_secs();
    let age = now.saturating_sub(s.last_check);
    let stale = s.last_check == 0 || s.last_check > now || age >= STALE_SECS;
    let b = |v: bool| if v { Row::Configured } else { Row::Attention };
    let dns = match &s.dns_status {
        DnsStatus::Unknown(_) => Row::Checking,
        DnsStatus::ForcedPlaintext => Row::OffHere,
        d => b(d.is_encrypted()),
    };
    let sandbox = match (&s.sandbox_status, &s.browser_sandbox) {
        (SandboxStatus::Unknown(_), _) | (_, BrowserSandbox::Unknown(_)) => Row::Checking,
        (st, br) => b(st.is_active() && br.is_isolated()),
    };
    let states = [dns, b(s.mac_randomization), b(s.hostname_privacy), b(s.ipv6_privacy), sandbox, b(s.firewall)];
    for (r, st) in rows.into_iter().zip(states) {
        r.set(if stale && st != Row::Checking { Row::Stale } else { st });
    }
    fresh.set_text(&if s.last_check == 0 {
        "Waiting for the privacy service's first check…".to_string()
    } else if stale {
        format!("Last checked {}. These may be out of date.", ago(age))
    } else {
        format!("Checked {}.", ago(age))
    });
    tech.set_text(&format!(
        "Service    org.arka.arkad (system D-Bus, read-only properties)\n\
         arkad::dns       DnsStatus = {}\n\
         arkad::mac       MacRandomization = {}\n\
         arkad::hostname  HostnamePrivacy = {}\n\
         arkad::ipv6      Ipv6Privacy = {}\n\
         arkad::sandbox   SandboxStatus = {}, BrowserSandbox = {}\n\
         LastCheck  {}",
        s.dns_status.as_str(), s.mac_randomization, s.hostname_privacy, s.ipv6_privacy,
        s.sandbox_status.as_str(), s.browser_sandbox.as_str(), s.last_check,
    ));
}

fn apply_updates(r: Result<UpdateStatus, String>, vals: &[gtk4::Label], err: &gtk4::Label) {
    match r {
        Ok(u) => {
            err.set_visible(false);
            let ver = |d: &arka_update::status::Deployment| {
                d.version.clone().or_else(|| d.image.clone()).unwrap_or_else(|| "Unknown".into())
            };
            vals[0].set_text(&ver(&u.installed));
            vals[1].set_text(&match &u.staged {
                Some(d) if d.download_only => format!("{} (downloaded, not yet applied)", ver(d)),
                Some(d) => ver(d),
                None => "None".into(),
            });
            vals[2].set_text(if u.restart_needed { "Yes" } else { "No" });
            vals[3].set_text(if u.rollback_queued {
                "Selected for the next restart"
            } else if u.rollback_available {
                "Available"
            } else {
                "None on this computer"
            });
        }
        Err(e) => {
            for v in vals { v.set_text("—"); }
            err.set_text(&format!("Couldn't read the update status ({e}). Open Updates to check."));
            err.set_visible(true);
        }
    }
}

// --- per-network encrypted-DNS exception --------------------------------------

/// The active network connection's NetworkManager name (not loopback/bridges).
fn active_connection() -> Option<String> {
    let out = std::process::Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show", "--active"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        let (name, ty) = l.rsplit_once(':')?;
        matches!(ty, "802-11-wireless" | "802-3-ethernet" | "gsm" | "wifi" | "ethernet")
            .then(|| name.replace("\\:", ":"))
    })
}

/// Allow (or stop allowing) unencrypted DNS on one connection. NetworkManager
/// asks for permission itself; arkad keeps every other network strict.
fn set_dns_exception(conn: &str, allow: bool) -> Result<(), String> {
    set_dns_exception_with(conn, allow, |args| {
        std::process::Command::new("nmcli").args(args)
            .status().map(|s| s.success()).unwrap_or(false)
    })
}

fn set_dns_exception_with(conn: &str, allow: bool, mut run: impl FnMut(&[&str]) -> bool) -> Result<(), String> {
    let value = if allow { "no" } else { "default" };
    if !run(&["connection", "modify", conn, "connection.dns-over-tls", value]) {
        return Err("Could not save the DNS setting. Recheck the connection settings before retrying.".into());
    }
    if !run(&["connection", "up", conn]) {
        return Err("The DNS setting was saved, but reconnecting failed. It will apply when this connection next activates. Check the network settings.".into());
    }
    Ok(())
}

#[cfg(test)]
mod dns_exception_tests {
    use super::set_dns_exception_with;
    #[test]
    fn failed_reconnect_reports_the_saved_change() {
        let mut calls = 0;
        let err = set_dns_exception_with("test", true, |_| { calls += 1; calls == 1 }).unwrap_err();
        assert_eq!(calls, 2);
        assert!(err.contains("was saved"));
        assert!(!err.contains("Nothing was changed"));
    }
    #[test]
    fn failed_save_does_not_reconnect() {
        let mut calls = 0;
        assert!(set_dns_exception_with("test", true, |_| { calls += 1; false }).is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn both_choices_save_before_reconnecting() {
        for (allow, value) in [(true, "no"), (false, "default")] {
            let mut calls = Vec::new();
            assert!(set_dns_exception_with("test", allow, |args| {
                calls.push(args.iter().map(|v| v.to_string()).collect::<Vec<_>>()); true
            }).is_ok());
            assert_eq!(calls[0], ["connection", "modify", "test", "connection.dns-over-tls", value]);
            assert_eq!(calls[1], ["connection", "up", "test"]);
        }
    }
}

type DnsTarget = std::rc::Rc<std::cell::RefCell<Option<(bool, String)>>>;

/// Does a name lookup through the system resolver work right now? A strict
/// encrypted-DNS configuration can be fully in place while the network blocks
/// it, so the configuration check alone can't tell.
fn lookups_work() -> bool {
    std::process::Command::new("timeout")
        .args(["6", "resolvectl", "query", "--legend=no", "--cache=no", "-t", "A", "fedoraproject.org"])
        .output().map(|o| o.status.success()).unwrap_or(false)
}

fn update_dns_box(s: &DashboardState, bx: &gtk4::Box, note: &gtk4::Label, btn: &gtk4::Button, target: &DnsTarget) {
    if !s.available || matches!(s.dns_status, DnsStatus::Unknown(_)) { bx.set_visible(false); return; }
    let forced = s.dns_status == DnsStatus::ForcedPlaintext;
    // A live lookup at most every 30 s, not on every refresh.
    thread_local!(static LAST: std::cell::Cell<(u64, bool)> = const { std::cell::Cell::new((0, false)) });
    let now = now_secs();
    if LAST.with(|l| { let (t, f) = l.get(); now.saturating_sub(t) < 30 && f == forced }) { return; }
    LAST.with(|l| l.set((now, forced)));
    let (bx, note, btn, target) = (bx.clone(), note.clone(), btn.clone(), target.clone());
    glib::spawn_future_local(async move {
        // Offer the exception only when lookups actually fail; offer to undo it
        // whenever it's in use.
        let (conn, failing) = gtk4::gio::spawn_blocking(move || {
            let conn = active_connection();
            let failing = !forced && conn.is_some() && !lookups_work();
            (conn, failing)
        }).await.unwrap_or((None, false));
        let mode = if forced { "restore" } else if failing { "allow" } else { bx.set_visible(false); return };
        let Some(conn) = conn else { bx.set_visible(false); return };
        if mode == "allow" {
            note.set_text(&format!(
                "Website names aren't resolving on \u{201c}{conn}\u{201d}. Some networks (hotels, cafés, \
                 sign-in pages) block encrypted DNS. You can allow unencrypted DNS on this network only; \
                 that network can then see which sites you look up."));
            btn.set_label("Allow on this network only");
        } else {
            note.set_text(&format!("Unencrypted DNS is allowed on \u{201c}{conn}\u{201d}."));
            btn.set_label("Require encryption here again");
        }
        *target.borrow_mut() = Some((mode == "allow", conn));
        bx.set_visible(true);
    });
}
