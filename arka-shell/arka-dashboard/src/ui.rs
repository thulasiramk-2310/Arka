use std::sync::mpsc::Sender;

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
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
    let c = if score >= 90 { "#4ade80" } else if score >= 70 { "#fbbf24" } else { "#f87171" };
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

    let brand = gtk4::Label::new(Some("▲  A R K A"));
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
        "Encrypted DNS",
        "DNS-over-TLS · Quad9 9.9.9.9:853",
        "network-server-symbolic",
    );
    let (mac_row, mac_badge) = make_action_row(
        "MAC Randomization",
        "Per-connection random hardware address",
        "network-wireless-symbolic",
    );
    let (host_row, host_badge) = make_action_row(
        "Hostname Privacy",
        "Static hostname \"arka\" — no device fingerprint",
        "computer-symbolic",
    );
    let (ipv6_row, ipv6_badge) = make_action_row(
        "IPv6 Privacy Extensions",
        "Temporary addresses per RFC 4941",
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
        "Browser Sandbox",
        "bubblewrap isolation · tmpfs home directory",
        "application-x-executable-symbolic",
    );
    let (bstatus_row, bstatus_badge) = make_action_row(
        "Sandbox Infrastructure",
        "bwrap wrapper binary present and active",
        "security-medium-symbolic",
    );

    browser_group.add(&bsandbox_row);
    browser_group.add(&bstatus_row);
    browser_group.add(&static_row(
        "Cookie Policy",
        "Cleared on session close",
        "emblem-documents-symbolic",
        "Per-Session",
    ));
    browser_group.add(&static_row(
        "Machine ID",
        "Hidden from browser sandbox via /etc allowlist",
        "emblem-system-symbolic",
        "Blocked",
    ));
    browser_group.add(&static_row(
        "WiFi Credentials",
        "NetworkManager config excluded from sandbox",
        "network-wireless-signal-excellent-symbolic",
        "Hidden",
    ));

    // ── score breakdown ──────────────────────────────────────────────────────
    let score_group = adw::PreferencesGroup::new();
    score_group.set_title("Privacy Score Breakdown");
    score_group.set_description(Some("100 points total across 8 privacy factors"));

    let (dns_srow, dns_sr)     = ScoreRow::new("DNS Encryption",         25);
    let (mac_srow, mac_sr)     = ScoreRow::new("MAC Randomization",      20);
    let (host_srow, host_sr)   = ScoreRow::new("Hostname Privacy",       10);
    let (ipv6_srow, ipv6_sr)   = ScoreRow::new("IPv6 Privacy",           10);
    let (brow_srow, brow_sr)   = ScoreRow::new("Browser Sandbox",        10);
    let (sand_srow, sand_sr)   = ScoreRow::new("Sandbox Infrastructure", 15);
    let (tele_srow, tele_sr)   = ScoreRow::new("Telemetry Blocked",       5);
    let (track_srow, track_sr) = ScoreRow::new("Tracking Blocked",        5);

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

    // ── assemble ─────────────────────────────────────────────────────────────
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_bottom(32);
    content.append(&header_card);
    content.append(&net_group);
    content.append(&browser_group);
    content.append(&score_group);
    content.append(&actions_group);

    let clamp = adw::Clamp::builder()
        .maximum_size(720)
        .tightening_threshold(640)
        .build();
    clamp.set_child(Some(&content));

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    scroll.set_child(Some(&clamp));

    let toolbar = adw::ToolbarView::new();
    let headerbar = adw::HeaderBar::new();
    headerbar.set_title_widget(Some(
        &adw::WindowTitle::new("Privacy Dashboard", "ArkaOS"),
    ));
    toolbar.add_top_bar(&headerbar);
    toolbar.set_content(Some(&scroll));

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
