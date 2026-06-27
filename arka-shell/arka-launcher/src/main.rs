mod apps;

use std::cell::RefCell;
use std::rc::Rc;

use arka_shell_common::theme;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, Image, Label,
    ListBox, ListBoxRow, Orientation, ScrolledWindow, SelectionMode,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const APP_ID: &str = "org.arka.launcher";

const STYLE: &str = "
window { background-color: alpha(@bg_sunken, 0.55); }

.launcher-card {
    background-color: @bg_raised;
    border-radius: 14px;
    border: 1px solid @border_ui;
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
    min-width: 560px;
    max-width: 560px;
}

/* Search row */
.search-row {
    padding: 16px 20px;
    border-bottom: 1px solid @border_sub;
}
.search-icon { color: @text_muted; margin-right: 10px; }
.search-input {
    background: transparent;
    border: none;
    outline: none;
    font-size: 16px;
    color: @text_hi;
    caret-color: @accent;
}
.search-input:focus { box-shadow: none; }
.esc-hint {
    font-size: 10px; font-weight: 600; color: @text_lo;
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 4px;
    padding: 3px 8px;
}

/* Section label */
.section-label {
    font-size: 10px; font-weight: 600; color: @text_muted;
    letter-spacing: 0.8px;
    padding: 12px 20px 4px;
}

/* App grid */
.apps-grid-wrap { padding: 4px 14px 10px; }
.app-grid-btn {
    background: transparent; border: none; box-shadow: none;
    border-radius: 8px;
    padding: 12px 4px;
    min-width: 88px;
}
.app-grid-btn:hover { background-color: @bg_overlay; }
.app-grid-btn image { color: @text_hi; }
.app-grid-name { font-size: 11px; color: @text_lo; margin-top: 8px; }

/* Results list */
.results-list { background: transparent; padding: 6px; }
.results-list row { background: transparent; border-radius: 6px; padding: 0; border: none; }
.results-list row:hover { background-color: @bg_overlay; }
.results-list row:selected { background-color: alpha(@accent, 0.10); }
.result-row { padding: 8px 12px; min-height: 36px; }
.result-icon { min-width: 28px; color: @text_lo; }
.result-title { font-size: 14px; color: @text_hi; font-weight: 500; }
.result-sub { font-size: 11px; color: @text_lo; }
.result-action { font-size: 11px; color: @accent; }

/* Power row */
.power-sep { background-color: @border_sub; margin: 0 16px; }
.power-row-box { padding: 10px 16px 12px; }
.power-btn {
    background: transparent;
    border: 1px solid @border_ui;
    border-radius: 6px;
    padding: 6px 12px;
    color: @text_lo; font-size: 12px;
    min-width: 96px;
}
.power-btn image { color: @text_lo; margin-right: 4px; }
.power-btn:hover { background-color: @bg_overlay; color: @text_hi; border-color: @border_emph; }
.power-btn:hover image { color: @text_hi; }
.power-btn.destruct:hover { background-color: alpha(@danger, 0.10); color: @danger; border-color: alpha(@danger, 0.30); }
.power-btn.destruct:hover image { color: @danger; }
";

const GRID_APPS: &[(&str, &str, &str)] = &[
    ("folder-symbolic",              "Files",     "thunar"),
    ("utilities-terminal-symbolic",  "Terminal",  "foot"),
    ("web-browser-symbolic",         "Firefox",   "firefox"),
    ("security-high-symbolic",       "Privacy",   "arka-dashboard"),
    ("application-x-addon-symbolic", "Capsule",   "arka-capsule"),
    ("emblem-system-symbolic",       "Settings",  "arka-settings-gtk"),
    ("bluetooth-symbolic",           "Bluetooth", "arka-bluetooth"),
    ("network-wireless-signal-good-symbolic", "Wi-Fi", "arka-wifi"),
    ("audio-volume-high-symbolic",   "Sound",     "arka-sound"),
    ("view-refresh-symbolic",        "Updates",   "arka-update"),
];

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let all_apps = apps::load();
    let filtered: Rc<RefCell<Vec<apps::AppEntry>>> = Rc::new(RefCell::new(all_apps.clone()));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("arka-launcher")
        .decorated(false)
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    // Fill the screen so the dimmed backdrop covers the whole desktop.
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    theme::install_base();
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_halign(gtk4::Align::Center);
    outer.set_valign(gtk4::Align::Center);

    let card = GtkBox::new(Orientation::Vertical, 0);
    card.add_css_class("launcher-card");

    // ── Search row ────────────────────────────────────────────────────────
    let search_row = GtkBox::new(Orientation::Horizontal, 0);
    search_row.add_css_class("search-row");
    search_row.set_valign(gtk4::Align::Center);

    let search_icon = Image::from_icon_name("system-search-symbolic");
    search_icon.set_pixel_size(16);
    search_icon.add_css_class("search-icon");

    let entry = Entry::new();
    entry.add_css_class("search-input");
    entry.set_placeholder_text(Some("Search apps, files, settings..."));
    entry.set_hexpand(true);
    entry.set_has_frame(false);

    let esc_hint = Label::new(Some("ESC"));
    esc_hint.add_css_class("esc-hint");

    search_row.append(&search_icon);
    search_row.append(&entry);
    search_row.append(&esc_hint);

    // ── Apps grid ─────────────────────────────────────────────────────────
    let apps_section_lbl = Label::new(Some("Applications"));
    apps_section_lbl.add_css_class("section-label");
    apps_section_lbl.set_halign(gtk4::Align::Start);

    let apps_wrap = GtkBox::new(Orientation::Horizontal, 0);
    apps_wrap.add_css_class("apps-grid-wrap");

    let apps_grid = gtk4::FlowBox::new();
    apps_grid.set_max_children_per_line(5);
    apps_grid.set_min_children_per_line(5);
    apps_grid.set_selection_mode(SelectionMode::None);
    apps_grid.set_hexpand(true);

    for (icon, name, cmd) in GRID_APPS {
        let btn = Button::new();
        btn.add_css_class("app-grid-btn");
        let vbox = GtkBox::new(Orientation::Vertical, 0);
        vbox.set_halign(gtk4::Align::Center);
        let icon_img = Image::from_icon_name(icon);
        icon_img.set_pixel_size(24);
        let name_lbl = Label::new(Some(name));
        name_lbl.add_css_class("app-grid-name");
        vbox.append(&icon_img);
        vbox.append(&name_lbl);
        btn.set_child(Some(&vbox));
        let cmd_owned = cmd.to_string();
        let win = window.clone();
        btn.connect_clicked(move |_| { launch(&cmd_owned); win.close(); });
        apps_grid.insert(&btn, -1);
    }
    apps_wrap.append(&apps_grid);

    // ── Results list ─────────────────────────────────────────────────────
    let results_lbl = Label::new(Some("Quick Actions"));
    results_lbl.add_css_class("section-label");
    results_lbl.set_halign(gtk4::Align::Start);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Browse);
    list.add_css_class("results-list");

    let scroll = ScrolledWindow::builder()
        .min_content_height(0)
        .max_content_height(240)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    scroll.set_child(Some(&list));

    // ── Power row ─────────────────────────────────────────────────────────
    let power_sep = gtk4::Separator::new(Orientation::Horizontal);
    power_sep.add_css_class("power-sep");

    let power_row = GtkBox::new(Orientation::Horizontal, 6);
    power_row.add_css_class("power-row-box");
    power_row.set_halign(gtk4::Align::Center);

    for (icon, lbl, cmd, destruct) in &[
        ("weather-clear-night-symbolic", "Sleep",    "systemctl suspend",  false),
        ("system-lock-screen-symbolic",  "Lock",     "swaylock",            false),
        ("view-refresh-symbolic",        "Restart",  "systemctl reboot",   false),
        ("system-shutdown-symbolic",     "Shutdown", "systemctl poweroff",  true),
    ] {
        let btn = Button::new();
        btn.add_css_class("power-btn");
        if *destruct { btn.add_css_class("destruct"); }
        let hb = GtkBox::new(Orientation::Horizontal, 0);
        hb.set_halign(gtk4::Align::Center);
        let img = Image::from_icon_name(icon);
        img.set_pixel_size(14);
        hb.append(&img);
        hb.append(&Label::new(Some(lbl)));
        btn.set_child(Some(&hb));
        let c = cmd.to_string();
        let win = window.clone();
        btn.connect_clicked(move |_| {
            win.close();
            let _ = std::process::Command::new("sh").args(["-c", &c]).spawn();
        });
        power_row.append(&btn);
    }

    card.append(&search_row);
    card.append(&apps_section_lbl);
    card.append(&apps_wrap);
    card.append(&results_lbl);
    card.append(&scroll);
    card.append(&power_sep);
    card.append(&power_row);

    outer.append(&card);
    outer.set_hexpand(true);
    outer.set_vexpand(true);
    window.set_child(Some(&outer));

    // Click on the dimmed backdrop (anywhere outside the card) dismisses.
    // A claiming gesture on the card swallows inner clicks so they don't bubble.
    let card_claim = gtk4::GestureClick::new();
    card_claim.connect_pressed(move |g, _, _, _| { g.set_state(gtk4::EventSequenceState::Claimed); });
    card.add_controller(card_claim);

    let backdrop_click = gtk4::GestureClick::new();
    let win_bd = window.clone();
    backdrop_click.connect_released(move |_, _, _, _| win_bd.close());
    outer.add_controller(backdrop_click);

    populate_results(&list, &all_apps, false);

    // Search → filter
    let all_apps2 = all_apps.clone();
    let list2 = list.clone();
    let filtered2 = filtered.clone();
    let apps_sec = apps_section_lbl.clone();
    let apps_wr = apps_wrap.clone();
    let res_lbl = results_lbl.clone();
    entry.connect_changed(move |e| {
        let q = e.text();
        let searching = !q.is_empty();
        apps_sec.set_visible(!searching);
        apps_wr.set_visible(!searching);
        res_lbl.set_text(if searching { "Results" } else { "Quick Actions" });
        let new_filtered: Vec<_> = if searching {
            all_apps2.iter().filter(|a| a.matches(&q)).cloned().collect()
        } else {
            all_apps2.clone()
        };
        populate_results(&list2, &new_filtered, searching);
        *filtered2.borrow_mut() = new_filtered;
    });

    // Enter → launch first
    let filtered3 = filtered.clone();
    let win3 = window.clone();
    entry.connect_activate(move |_| {
        let fa = filtered3.borrow();
        if let Some(first) = fa.first() { launch(&first.exec); win3.close(); }
    });

    // Row click
    let win4 = window.clone();
    list.connect_row_activated(move |_, row| {
        if let Some(exec) = row.widget_name().as_str().strip_prefix("exec:") {
            launch(exec);
            win4.close();
        }
    });

    // ESC
    let controller = gtk4::EventControllerKey::new();
    let win5 = window.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape { win5.close(); return glib::Propagation::Stop; }
        glib::Propagation::Proceed
    });
    window.add_controller(controller);

    window.present();
    entry.grab_focus();
}

fn populate_results(list: &ListBox, apps: &[apps::AppEntry], searching: bool) {
    while let Some(child) = list.first_child() { list.remove(&child); }

    if !searching {
        let quick: &[(&str, &str, &str, &str)] = &[
            ("security-high-symbolic", "Privacy Dashboard",  "Score: all protections active", "arka-dashboard"),
            ("view-refresh-symbolic",  "Check for Updates",  "bootc upgrade · atomic OTA",    "arka-update"),
            ("preferences-desktop-keyboard-symbolic", "Keyboard Shortcuts", "Super+D / Super+W / Super+T …", "arka-hotkeys"),
        ];
        for (icon, title, sub, cmd) in quick {
            let row = make_result_row(icon, title, sub, "Open →", &format!("exec:{cmd}"));
            list.append(&row);
        }
        return;
    }

    for app in apps.iter().take(6) {
        let icon = app_icon_symbolic(&app.name);
        let row = make_result_row(icon, &app.name, &app.exec, "Launch →", &format!("exec:{}", app.exec));
        list.append(&row);
    }
}

fn make_result_row(icon: &str, title: &str, sub: &str, action: &str, name: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    if !name.is_empty() { row.set_widget_name(name); }
    let hbox = GtkBox::new(Orientation::Horizontal, 10);
    hbox.add_css_class("result-row");
    let icon_lbl = Image::from_icon_name(icon);
    icon_lbl.set_pixel_size(18);
    icon_lbl.add_css_class("result-icon");
    icon_lbl.set_valign(gtk4::Align::Center);
    let vbox = GtkBox::new(Orientation::Vertical, 1);
    vbox.set_hexpand(true);
    let t = Label::new(Some(title));
    t.add_css_class("result-title");
    t.set_halign(gtk4::Align::Start);
    let s = Label::new(Some(sub));
    s.add_css_class("result-sub");
    s.set_halign(gtk4::Align::Start);
    vbox.append(&t);
    vbox.append(&s);
    let a = Label::new(Some(action));
    a.add_css_class("result-action");
    a.set_valign(gtk4::Align::Center);
    hbox.append(&icon_lbl);
    hbox.append(&vbox);
    hbox.append(&a);
    row.set_child(Some(&hbox));
    row
}

fn app_icon_symbolic(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("privacy") || n.contains("dashboard") { return "security-high-symbolic"; }
    if n.contains("wifi") || n.contains("wi-fi")        { return "network-wireless-signal-good-symbolic"; }
    if n.contains("bluetooth")                           { return "bluetooth-symbolic"; }
    if n.contains("sound") || n.contains("volume")      { return "audio-volume-high-symbolic"; }
    if n.contains("update")                              { return "view-refresh-symbolic"; }
    if n.contains("setting")                             { return "emblem-system-symbolic"; }
    if n.contains("file") || n.contains("thunar")       { return "folder-symbolic"; }
    if n.contains("firefox") || n.contains("browser")   { return "web-browser-symbolic"; }
    if n.contains("terminal") || n.contains("foot")     { return "utilities-terminal-symbolic"; }
    if n.contains("capsule") || n.contains("store")     { return "application-x-addon-symbolic"; }
    if n.contains("perm")                                { return "dialog-password-symbolic"; }
    if n.contains("hotkey")                              { return "preferences-desktop-keyboard-symbolic"; }
    if n.contains("welcome")                             { return "user-available-symbolic"; }
    "application-x-executable-symbolic"
}

fn launch(exec: &str) {
    let parts: Vec<&str> = exec.split_whitespace().collect();
    if parts.is_empty() { return; }
    let _ = std::process::Command::new(parts[0]).args(&parts[1..]).spawn();
}
