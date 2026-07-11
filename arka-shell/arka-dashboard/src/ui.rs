use std::sync::mpsc::Sender;

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::dbus::call_enforce_all;
use crate::state::{DashboardState, StateUpdate};

const CSS: &str = include_str!("style.css");

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

fn score_markup(score: u8) -> String {
    let c = if score >= 90 { "#22c55e" } else if score >= 70 { "#f59e0b" } else { "#ef4444" };
    format!("<span foreground='{c}' font_size='xx-large' font_weight='900'>{score}</span>")
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

fn make_action_row(title: &str, subtitle: &str, icon: &str) -> (adw::ActionRow, gtk4::Label) {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.add_prefix(&make_icon(icon));
    row.set_activatable(false);

    let badge = gtk4::Label::new(Some("…"));
    badge.set_valign(gtk4::Align::Center);
    badge.add_css_class("badge-yellow");
    row.add_suffix(&badge);

    (row, badge)
}

fn static_row(title: &str, subtitle: &str, icon: &str, badge_text: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.add_prefix(&make_icon(icon));
    row.set_activatable(false);

    let badge = gtk4::Label::new(Some(badge_text));
    badge.set_valign(gtk4::Align::Center);
    badge.add_css_class("badge-green");
    row.add_suffix(&badge);
    row
}

struct ScoreRow {
    bar:   gtk4::LevelBar,
    pts:   gtk4::Label,
    check: gtk4::Image,
}

impl ScoreRow {
    fn new(title: &str, max: u8) -> (adw::ActionRow, Self) {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_activatable(false);

        let check = gtk4::Image::from_icon_name("emblem-ok-symbolic");
        check.set_pixel_size(16);
        check.add_css_class("score-check-pending");
        row.add_prefix(&check);

        let bar = gtk4::LevelBar::new();
        bar.set_min_value(0.0);
        bar.set_max_value(max as f64);
        bar.set_value(0.0);
        bar.set_size_request(130, -1);
        bar.set_valign(gtk4::Align::Center);
        bar.add_css_class("score-bar");
        row.add_suffix(&bar);

        let pts = gtk4::Label::new(Some(&format!("0/{max}")));
        pts.set_width_chars(6);
        pts.set_halign(gtk4::Align::End);
        pts.add_css_class("dim-label");
        row.add_suffix(&pts);

        (row, ScoreRow { bar, pts, check })
    }

    fn update(&self, earned: u8, max: u8) {
        self.bar.set_value(earned as f64);
        self.pts.set_text(&format!("{earned}/{max}"));
        if earned > 0 {
            self.check.set_icon_name(Some("emblem-ok-symbolic"));
            self.check.remove_css_class("score-check-pending");
            self.check.remove_css_class("score-check-bad");
            self.check.add_css_class("score-check-good");
        } else {
            self.check.set_icon_name(Some("dialog-warning-symbolic"));
            self.check.remove_css_class("score-check-pending");
            self.check.remove_css_class("score-check-good");
            self.check.add_css_class("score-check-bad");
        }
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

fn event_color(ev: &str) -> &'static str {
    match ev {
        "drift"     => "#f59e0b",
        "recovered" => "#22c55e",
        "started" | "ready" | "active" => "#3b82f6",
        _           => "#7d7d8a",
    }
}

/// "This week" report card: aggregates the last 7 days of privacy.jsonl into
/// the numbers a user actually cares about — protections enforced, drifts
/// caught and fixed, sandboxed launches, and whether DNS stayed encrypted.
fn weekly_report_card(all: &[LogEvent]) -> adw::PreferencesGroup {
    let now = glib::DateTime::now_local()
        .map(|d| d.to_unix() as u64)
        .unwrap_or(0);
    let week_ago = now.saturating_sub(7 * 24 * 3600);
    let week: Vec<&LogEvent> = all.iter().filter(|e| e.ts >= week_ago).collect();

    let drifts    = week.iter().filter(|e| e.ev == "drift").count();
    let recovered = week.iter().filter(|e| e.ev == "recovered").count();
    let sandboxed = week.iter().filter(|e| e.cat == "sandbox").count();
    let dns_ok    = !week.iter().any(|e| e.cat == "dns" && e.ev == "drift");

    let group = adw::PreferencesGroup::new();
    group.set_title("This Week");
    group.set_description(Some("Privacy report · last 7 days"));

    let stats: [(&str, String, &str); 4] = [
        ("Privacy events", week.len().to_string(), "view-list-symbolic"),
        (
            "Drifts caught &amp; fixed",
            if drifts == 0 { "0 — clean".into() } else { format!("{drifts} caught / {recovered} fixed") },
            "security-high-symbolic",
        ),
        ("Sandboxed launches", sandboxed.to_string(), "system-lock-screen-symbolic"),
        (
            "Encrypted DNS",
            if dns_ok { "100% (DoT · Quad9)".into() } else { "interrupted — re-enforced".into() },
            "network-server-symbolic",
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

        let color = event_color(&ev.ev);
        let css = gtk4::CssProvider::new();
        css.load_from_data(&format!("image {{ color: {color}; }}"));
        icon.style_context().add_provider(&css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);

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

    // ── header card ─────────────────────────────────────────────────────────
    let header_card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    header_card.set_halign(gtk4::Align::Center);
    header_card.set_margin_top(32);
    header_card.set_margin_bottom(8);

    let brand = gtk4::Label::new(Some("A R K A"));
    brand.add_css_class("arka-brand");

    let score_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    score_row.set_halign(gtk4::Align::Center);

    let score_num = gtk4::Label::new(None);
    score_num.set_use_markup(true);
    score_num.set_markup(&score_markup(0));

    let score_denom = gtk4::Label::new(Some("/ 100"));
    score_denom.add_css_class("score-denom");
    score_denom.set_valign(gtk4::Align::End);

    score_row.append(&score_num);
    score_row.append(&score_denom);

    let tagline = gtk4::Label::new(Some("Your Computer Is Yours"));
    tagline.add_css_class("score-tagline");

    header_card.append(&brand);
    header_card.append(&score_row);
    header_card.append(&tagline);

    // ── network section ──────────────────────────────────────────────────────
    let net_group = adw::PreferencesGroup::new();
    net_group.set_title("Network Privacy");

    let (dns_row, dns_badge) = make_action_row(
        "Searches are private",
        "Your internet provider cannot see what you look up",
        "network-server-symbolic",
    );
    let (mac_row, mac_badge) = make_action_row(
        "Device can't be tracked by hardware",
        "Your network identity changes on every connection",
        "network-wireless-symbolic",
    );
    let (host_row, host_badge) = make_action_row(
        "Anonymous on local networks",
        "Nearby devices can't identify your computer",
        "computer-symbolic",
    );
    let (ipv6_row, ipv6_badge) = make_action_row(
        "IP address rotates automatically",
        "Your home address on the internet changes regularly",
        "preferences-system-network-symbolic",
    );
    net_group.add(&dns_row);
    net_group.add(&mac_row);
    net_group.add(&host_row);
    net_group.add(&ipv6_row);

    // ── browser section ──────────────────────────────────────────────────────
    let browser_group = adw::PreferencesGroup::new();
    browser_group.set_title("Browser");

    let (bsandbox_row, bsandbox_badge) = make_action_row(
        "Browser can't access your files",
        "Firefox runs in an isolated container, away from your data",
        "application-x-executable-symbolic",
    );
    let (bstatus_row, bstatus_badge) = make_action_row(
        "App containment is active",
        "The security wrapper is running and protecting you",
        "security-medium-symbolic",
    );

    browser_group.add(&bsandbox_row);
    browser_group.add(&bstatus_row);
    browser_group.add(&static_row(
        "No tracking cookies",
        "Cookies are deleted when you close the browser",
        "emblem-documents-symbolic",
        "Per-Session",
    ));
    browser_group.add(&static_row(
        "Computer ID is hidden",
        "Websites can't fingerprint your machine's identity",
        "emblem-system-symbolic",
        "Hidden",
    ));
    browser_group.add(&static_row(
        "WiFi passwords are protected",
        "Your saved networks are invisible to the browser",
        "network-wireless-signal-excellent-symbolic",
        "Protected",
    ));

    // ── score breakdown ──────────────────────────────────────────────────────
    let score_group = adw::PreferencesGroup::new();
    score_group.set_title("Privacy Score Breakdown");
    score_group.set_description(Some("100 points total across 8 privacy factors"));

    let (dns_srow, dns_sr)     = ScoreRow::new("Private Searches",    25);
    let (mac_srow, mac_sr)     = ScoreRow::new("Hidden Device ID",     20);
    let (host_srow, host_sr)   = ScoreRow::new("Network Anonymity",    10);
    let (ipv6_srow, ipv6_sr)   = ScoreRow::new("Rotating IP Address",  10);
    let (brow_srow, brow_sr)   = ScoreRow::new("Browser Isolation",    10);
    let (sand_srow, sand_sr)   = ScoreRow::new("App Containment",      15);
    let (tele_srow, tele_sr)   = ScoreRow::new("No Telemetry",          5);
    let (track_srow, track_sr) = ScoreRow::new("No Tracking",           5);

    score_group.add(&dns_srow);
    score_group.add(&mac_srow);
    score_group.add(&host_srow);
    score_group.add(&ipv6_srow);
    score_group.add(&brow_srow);
    score_group.add(&sand_srow);
    score_group.add(&tele_srow);
    score_group.add(&track_srow);

    // ── quick actions ────────────────────────────────────────────────────────
    let actions_group = adw::PreferencesGroup::new();
    actions_group.set_title("Quick Actions");

    let actions_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    actions_box.set_margin_top(8);
    actions_box.set_margin_bottom(8);
    actions_box.set_halign(gtk4::Align::Center);

    let fix_btn = gtk4::Button::with_label("Fix All");
    fix_btn.add_css_class("suggested-action");
    fix_btn.add_css_class("pill");

    let harden_btn = gtk4::Button::with_label("Harden Browser");
    harden_btn.add_css_class("pill");
    harden_btn.set_sensitive(false);

    let logs_btn = gtk4::Button::with_label("View Logs");
    logs_btn.add_css_class("pill");
    logs_btn.set_sensitive(false);

    actions_box.append(&fix_btn);
    actions_box.append(&harden_btn);
    actions_box.append(&logs_btn);
    actions_group.add(&actions_box);

    // ── status page ──────────────────────────────────────────────────────────
    let status_content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    status_content.set_margin_start(16);
    status_content.set_margin_end(16);
    status_content.set_margin_bottom(32);
    status_content.append(&header_card);
    status_content.append(&net_group);
    status_content.append(&browser_group);
    status_content.append(&score_group);
    status_content.append(&actions_group);

    let status_clamp = adw::Clamp::builder()
        .maximum_size(720)
        .tightening_threshold(640)
        .build();
    status_clamp.set_child(Some(&status_content));

    let status_scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    status_scroll.set_child(Some(&status_clamp));

    // ── timeline page ────────────────────────────────────────────────────────
    let timeline_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    timeline_content.set_margin_start(16);
    timeline_content.set_margin_end(16);
    timeline_content.set_margin_top(16);
    timeline_content.set_margin_bottom(16);
    populate_timeline(&timeline_content);

    let timeline_clamp = adw::Clamp::builder()
        .maximum_size(720)
        .tightening_threshold(640)
        .build();
    timeline_clamp.set_child(Some(&timeline_content));

    let timeline_scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    timeline_scroll.set_child(Some(&timeline_clamp));

    // ── view stack ───────────────────────────────────────────────────────────
    let stack = adw::ViewStack::new();
    let status_page = stack.add_titled(&status_scroll, Some("status"), "Status");
    status_page.set_icon_name(Some("security-high-symbolic"));
    let timeline_page = stack.add_titled(&timeline_scroll, Some("timeline"), "Timeline");
    timeline_page.set_icon_name(Some("document-open-recent-symbolic"));

    // Refresh timeline on tab switch
    let tc_ref = timeline_content.clone();
    stack.connect_visible_child_notify(move |s| {
        if s.visible_child_name().as_deref() == Some("timeline") {
            populate_timeline(&tc_ref);
        }
    });

    // ── switcher bar ─────────────────────────────────────────────────────────
    let switcher_bar = adw::ViewSwitcherBar::new();
    switcher_bar.set_stack(Some(&stack));
    switcher_bar.set_reveal(true);

    // Keyboard access: the switcher bar isn't in the focus chain, so give the
    // tabs shortcuts (Ctrl+1 Status, Ctrl+2 Timeline).
    let keys = gtk4::EventControllerKey::new();
    let stack_keys = stack.clone();
    keys.connect_key_pressed(move |_, key, _, modifier| {
        if modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
            match key {
                gtk4::gdk::Key::_1 => {
                    stack_keys.set_visible_child_name("status");
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::_2 => {
                    stack_keys.set_visible_child_name("timeline");
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);

    // ── assemble ─────────────────────────────────────────────────────────────
    let toolbar = adw::ToolbarView::new();
    let headerbar = adw::HeaderBar::new();

    // Refresh button (visible when on Timeline)
    let refresh_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.set_tooltip_text(Some("Refresh timeline"));
    let tc_ref2 = timeline_content.clone();
    refresh_btn.connect_clicked(move |_| {
        populate_timeline(&tc_ref2);
    });
    headerbar.pack_start(&refresh_btn);

    headerbar.set_title_widget(Some(
        &adw::WindowTitle::new("Privacy Dashboard", "ArkaOS"),
    ));
    toolbar.add_top_bar(&headerbar);
    toolbar.set_content(Some(&stack));
    toolbar.add_bottom_bar(&switcher_bar);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar));
    window.set_content(Some(&toast_overlay));

    // Fix All button
    let tx_fix = tx;
    fix_btn.connect_clicked(move |_| {
        call_enforce_all(tx_fix.clone());
    });

    // ── updater closure ───────────────────────────────────────────────────────
    Box::new(move |update: StateUpdate| match update {
        StateUpdate::Full(s) => apply_state(
            &s,
            &score_num,
            &dns_badge, &mac_badge, &host_badge, &ipv6_badge,
            &bsandbox_badge, &bstatus_badge,
            &dns_sr, &mac_sr, &host_sr, &ipv6_sr,
            &brow_sr, &sand_sr, &tele_sr, &track_sr,
        ),
        StateUpdate::EnforceResult(r) => {
            let msg = if r.is_ok() {
                "All privacy controls enforced"
            } else {
                "Enforcement failed — check arkad logs"
            };
            toast_overlay.add_toast(adw::Toast::new(msg));
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_state(
    s: &DashboardState,
    score_num: &gtk4::Label,
    dns_badge: &gtk4::Label,
    mac_badge: &gtk4::Label,
    host_badge: &gtk4::Label,
    ipv6_badge: &gtk4::Label,
    bsandbox_badge: &gtk4::Label,
    bstatus_badge: &gtk4::Label,
    dns_sr:   &ScoreRow,
    mac_sr:   &ScoreRow,
    host_sr:  &ScoreRow,
    ipv6_sr:  &ScoreRow,
    brow_sr:  &ScoreRow,
    sand_sr:  &ScoreRow,
    tele_sr:  &ScoreRow,
    track_sr: &ScoreRow,
) {
    score_num.set_markup(&score_markup(s.privacy_score));

    let dns_good = s.dns_status == DnsStatus::Encrypted;
    let dns_warn = matches!(s.dns_status, DnsStatus::Degraded);
    set_badge(dns_badge, s.dns_status.as_str(), dns_good, dns_warn);

    set_badge(mac_badge,
        if s.mac_randomization { "Enabled" } else { "Disabled" },
        s.mac_randomization, false);

    set_badge(host_badge,
        if s.hostname_privacy { "Locked" } else { "Exposed" },
        s.hostname_privacy, false);

    set_badge(ipv6_badge,
        if s.ipv6_privacy { "Enabled" } else { "Disabled" },
        s.ipv6_privacy, false);

    let bsandbox_good = matches!(
        s.browser_sandbox,
        BrowserSandbox::Persistent | BrowserSandbox::Ephemeral | BrowserSandbox::PrivateWorkspace
    );
    set_badge(bsandbox_badge, s.browser_sandbox.as_str(), bsandbox_good, false);

    let bstatus_good = s.sandbox_status == SandboxStatus::Active;
    let bstatus_warn = s.sandbox_status == SandboxStatus::Partial;
    set_badge(bstatus_badge, s.sandbox_status.as_str(), bstatus_good, bstatus_warn);

    dns_sr.update(  if dns_good { 25 } else { 0 }, 25);
    mac_sr.update(  if s.mac_randomization { 20 } else { 0 }, 20);
    host_sr.update( if s.hostname_privacy { 10 } else { 0 }, 10);
    ipv6_sr.update( if s.ipv6_privacy { 10 } else { 0 }, 10);
    brow_sr.update( if bsandbox_good { 10 } else { 0 }, 10);
    sand_sr.update( if bstatus_good { 15 } else { 0 }, 15);
    tele_sr.update( if s.telemetry_blocked { 5 } else { 0 }, 5);
    track_sr.update(if s.tracking_blocked  { 5 } else { 0 }, 5);
}
