use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use arka_shell_common::window_service;

const APP_ID: &str = "org.arka.capsule";

// Smart search entries: (keywords, icon, display name, subtitle, cmd)
const SMART_ACTIONS: &[(&str, &str, &str, &str, &str)] = &[
    // Settings pages
    ("privacy security protection dns mac",    "security-high-symbolic",       "Privacy Settings",   "Searches protected · Browser isolated",     "arka-settings-gtk"),
    ("appearance wallpaper theme dark",        "applications-graphics-symbolic","Appearance",         "Wallpaper · Dark mode",                     "arka-settings-gtk"),
    ("wifi internet network wireless",         "network-wireless-signal-good-symbolic", "Wi-Fi Settings", "Connect to a network",               "arka-wifi"),
    ("bluetooth headphone mouse keyboard",     "bluetooth-symbolic",           "Bluetooth",          "Pair and connect devices",                  "arka-bluetooth"),
    ("sound volume speaker microphone",        "audio-volume-high-symbolic",   "Sound",              "Volume · Microphone · Output device",       "arka-sound"),
    ("update upgrade system software",         "view-refresh-symbolic",        "System Updates",     "Check and install updates",                 "arka-update"),
    ("general login about version hostname",   "emblem-system-symbolic",       "General Settings",   "Login · About · Hostname",                  "arka-settings-gtk"),
    // Quick actions
    ("lock screen security",                    "system-lock-screen-symbolic",  "Lock Screen",        "Lock your session now",                     "loginctl lock-session"),
    ("sleep suspend rest hibernate",           "weather-clear-night-symbolic", "Sleep",              "Suspend the computer",                      "systemctl suspend"),
    ("reboot restart",                         "view-refresh-symbolic",        "Restart",            "Reboot the computer",                       "systemctl reboot"),
    ("shutdown poweroff turn off",             "system-shutdown-symbolic",     "Shutdown",           "Turn off the computer",                     "systemctl poweroff"),
    // Privacy quick-opens
    ("permission permission app access camera mic",  "dialog-password-symbolic", "App Permissions", "Review what apps can access",              "arka-perms"),
    ("hotkey keyboard shortcut",               "preferences-desktop-keyboard-symbolic", "Keyboard Shortcuts", "Super+D, Super+B, Alt+F4…",      "arka-hotkeys"),
    ("privacy dashboard score protection",     "security-high-symbolic",       "Privacy Dashboard",  "See your privacy score",                    "arka-dashboard"),
];

// ArkaOS built-in apps
const SYSTEM_APPS: &[(&str, &str, &str)] = &[
    ("folder-symbolic",                       "Files",           "dolphin"),
    ("utilities-terminal-symbolic",           "Terminal",        "konsole"),
    ("web-browser-symbolic",                  "Firefox",         "firefox"),
    ("security-high-symbolic",                "Privacy",         "arka-dashboard"),
    ("emblem-system-symbolic",                "Settings",        "arka-settings-gtk"),
    ("bluetooth-symbolic",                    "Bluetooth",       "arka-bluetooth"),
    ("network-wireless-signal-good-symbolic", "Wi-Fi",           "arka-wifi"),
    ("audio-volume-high-symbolic",            "Sound",           "arka-sound"),
    ("view-refresh-symbolic",                 "Updates",         "arka-update"),
    ("dialog-password-symbolic",              "Permissions",     "arka-perms"),
];

#[derive(Clone)]
struct CatalogApp {
    name:        &'static str,
    id:          &'static str,
    description: &'static str,
    icon:        &'static str,
    category:    &'static str,
}

const CATALOG: &[CatalogApp] = &[
    CatalogApp { name: "Signal",      id: "org.signal.Signal",             description: "Private, encrypted messenger",             icon: "chat-symbolic",                     category: "Privacy"      },
    CatalogApp { name: "Bitwarden",   id: "com.bitwarden.desktop",         description: "Open-source password manager",             icon: "dialog-password-symbolic",          category: "Privacy"      },
    CatalogApp { name: "KeePassXC",   id: "org.keepassxc.KeePassXC",       description: "Local password vault — no cloud",          icon: "dialog-password-symbolic",          category: "Privacy"      },
    CatalogApp { name: "ProtonVPN",   id: "com.protonvpn.www",             description: "Privacy-first VPN",                        icon: "network-vpn-symbolic",              category: "Privacy"      },
    CatalogApp { name: "VLC",         id: "org.videolan.VLC",              description: "Play any video or audio file",             icon: "multimedia-player-symbolic",        category: "Media"        },
    CatalogApp { name: "Spotify",     id: "com.spotify.Client",            description: "Music streaming",                          icon: "audio-x-generic-symbolic",          category: "Media"        },
    CatalogApp { name: "LibreOffice", id: "org.libreoffice.LibreOffice",   description: "Full office suite — free and open",        icon: "x-office-document-symbolic",        category: "Productivity" },
    CatalogApp { name: "Thunderbird", id: "org.mozilla.Thunderbird",       description: "Email with encryption support",            icon: "mail-unread-symbolic",              category: "Productivity" },
    CatalogApp { name: "OnlyOffice",  id: "org.onlyoffice.desktopeditors", description: "Office suite compatible with .docx/.xlsx", icon: "x-office-spreadsheet-symbolic",     category: "Productivity" },
    CatalogApp { name: "GIMP",        id: "org.gimp.GIMP",                 description: "Professional image editor",                icon: "image-x-generic-symbolic",          category: "Creative"     },
    CatalogApp { name: "Inkscape",    id: "org.inkscape.Inkscape",         description: "Vector graphics editor",                   icon: "image-x-generic-symbolic",          category: "Creative"     },
    CatalogApp { name: "OBS Studio",  id: "com.obsproject.Studio",         description: "Screen recording and streaming",           icon: "video-x-generic-symbolic",          category: "Creative"     },
    CatalogApp { name: "Blender",     id: "org.blender.Blender",           description: "3D modelling and animation",               icon: "image-x-generic-symbolic",          category: "Creative"     },
    CatalogApp { name: "VSCodium",    id: "com.vscodium.codium",           description: "Code editor — no Microsoft telemetry",     icon: "utilities-terminal-symbolic",       category: "Developer"    },
    CatalogApp { name: "Bottles",     id: "com.usebottles.bottles",        description: "Run Windows apps in a container",          icon: "application-x-executable-symbolic", category: "Tools"       },
];

const STYLE: &str = "
/* Shared */
.section-title {
    font-size: 11px; font-weight: 700; color: @text_muted;
    letter-spacing: 1px;
    padding: 12px 16px 6px;
}

/* System app buttons */
.sys-app-btn {
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 8px;
    padding: 12px 8px 8px;
    min-width: 80px;
}
.sys-app-btn:hover { background-color: @bg_sunken; border-color: @border_emph; }
.sys-app-btn image { color: @text_hi; }
.sys-app-name { font-size: 11px; color: @text_lo; margin-top: 8px; }

/* Catalog cards */
.app-card {
    background-color: @bg_overlay;
    border-radius: 10px;
    border: 1px solid @border_ui;
    padding: 12px;
}
.app-card image { color: @text_hi; }
.app-name { font-size: 13px; font-weight: 700; color: @text_hi; }
.app-desc { font-size: 11px; color: @text_lo; }
.cat-pill {
    background-color: @bg_sunken;
    border: 1px solid @border_ui;
    border-radius: 999px; padding: 2px 10px;
    color: @text_lo; font-size: 10px;
}
.installed-btn { color: @accent; font-size: 12px; }

/* Running windows */
.win-row {
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 10px;
    padding: 10px 14px;
    margin: 3px 12px;
}
.win-row:hover { background-color: @bg_sunken; border-color: @border_emph; }
.win-title { font-size: 13px; color: @text_hi; font-weight: 500; }
.win-class { font-size: 11px; color: @text_muted; }
.win-focus-btn {
    background-color: alpha(@accent, 0.12); border: 1px solid alpha(@accent, 0.25);
    border-radius: 6px; color: @accent; font-size: 12px; padding: 3px 10px;
}
.win-close-btn {
    background: transparent; border: 1px solid alpha(@danger, 0.20);
    border-radius: 6px; color: @danger; font-size: 12px; padding: 3px 8px;
}
.empty-label { font-size: 14px; color: @text_muted; margin: 60px 20px; }

/* Favorites */
.fav-app-btn {
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 8px;
    padding: 12px 8px 8px;
    min-width: 80px;
}
.fav-app-btn:hover { background-color: @bg_sunken; border-color: @border_emph; }
.fav-app-btn image { color: @text_hi; }
.star-btn {
    background: transparent; border: none;
    font-size: 14px; padding: 4px 8px;
    color: @warn;
    border-radius: 6px;
}
.star-btn:hover { background-color: alpha(@warn, 0.10); }
.result-action { font-size: 13px; color: @accent; }
";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    arka_shell_common::theme::install_adw();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Capsule")
        .default_width(720)
        .default_height(640)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let favorites: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(load_favorites()));
    let installed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(flatpak_installed()));

    // ── Stack + tab switcher ────────────────────────────────────────────────
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);

    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&switcher));

    // ── Tab 1: Apps ─────────────────────────────────────────────────────────
    let apps_page = build_apps_tab(&installed, &favorites);
    stack.add_titled(&apps_page, Some("apps"), "Apps");

    // ── Tab 2: Running ──────────────────────────────────────────────────────
    let (running_page, running_list) = build_running_tab();
    stack.add_titled(&running_page, Some("running"), "Running");

    // Populate once now; thereafter refresh only when the Running tab is
    // shown (see the visible-child handler below). Each list() drives a KWin
    // script, so polling on a timer would spam the compositor.
    refresh_running(&running_list);

    // ── Tab 3: Favorites ────────────────────────────────────────────────────
    let (fav_page, fav_grid) = build_favorites_tab(&favorites);
    stack.add_titled(&fav_page, Some("favorites"), "Favorites");

    // Refresh a tab's contents when the user switches to it.
    stack.connect_visible_child_notify({
        let fav_grid = fav_grid.clone();
        let favorites = favorites.clone();
        let running_list = running_list.clone();
        move |s| match s.visible_child_name().as_deref() {
            Some("favorites") => repopulate_favorites(&fav_grid, &favorites.borrow()),
            Some("running") => refresh_running(&running_list),
            _ => {}
        }
    });

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&stack));
    window.set_content(Some(&toolbar));
    window.present();
}

// ── Tab 1: Apps ─────────────────────────────────────────────────────────────

fn build_apps_tab(
    installed: &Rc<RefCell<Vec<String>>>,
    favorites: &Rc<RefCell<Vec<String>>>,
) -> gtk4::Box {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    // Search bar
    let search = gtk4::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps…"));
    search.set_margin_start(16); search.set_margin_end(16);
    search.set_margin_top(10); search.set_margin_bottom(4);
    page.append(&search);

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    // Section: Smart Actions (shown only when searching)
    let smart_title = gtk4::Label::new(Some("Quick Actions"));
    smart_title.add_css_class("section-title");
    smart_title.set_halign(gtk4::Align::Start);
    smart_title.set_visible(false);
    content.append(&smart_title);

    let smart_list = gtk4::ListBox::new();
    smart_list.set_selection_mode(gtk4::SelectionMode::None);
    smart_list.add_css_class("boxed-list");
    smart_list.set_margin_start(12); smart_list.set_margin_end(12);
    smart_list.set_margin_bottom(10);
    smart_list.set_visible(false);
    content.append(&smart_list);

    // Section: ArkaOS Apps
    let sys_title = gtk4::Label::new(Some("ArkaOS Apps"));
    sys_title.add_css_class("section-title");
    sys_title.set_halign(gtk4::Align::Start);
    content.append(&sys_title);

    let sys_flow = gtk4::FlowBox::new();
    sys_flow.set_max_children_per_line(5);
    sys_flow.set_min_children_per_line(3);
    sys_flow.set_selection_mode(gtk4::SelectionMode::None);
    sys_flow.set_column_spacing(6);
    sys_flow.set_row_spacing(6);
    sys_flow.set_margin_start(12); sys_flow.set_margin_end(12);
    sys_flow.set_margin_bottom(10);

    for (icon, name, cmd) in SYSTEM_APPS {
        let btn = gtk4::Button::new();
        btn.add_css_class("sys-app-btn");
        let vb = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        vb.set_halign(gtk4::Align::Center);
        let icon_img = gtk4::Image::from_icon_name(icon);
        icon_img.set_pixel_size(24);
        let name_lbl = gtk4::Label::new(Some(name));
        name_lbl.add_css_class("sys-app-name");
        vb.append(&icon_img);
        vb.append(&name_lbl);
        btn.set_child(Some(&vb));
        let cmd = cmd.to_string();
        btn.connect_clicked(move |_| launch(&cmd));
        sys_flow.insert(&btn, -1);
    }
    content.append(&sys_flow);

    // Separator
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_start(16); sep.set_margin_end(16); sep.set_margin_bottom(4);
    content.append(&sep);

    // Section: Install Apps (catalog)
    let cat_title = gtk4::Label::new(Some("Install Apps"));
    cat_title.add_css_class("section-title");
    cat_title.set_halign(gtk4::Align::Start);
    content.append(&cat_title);

    let catalog_flow = gtk4::FlowBox::new();
    catalog_flow.set_max_children_per_line(3);
    catalog_flow.set_min_children_per_line(2);
    catalog_flow.set_selection_mode(gtk4::SelectionMode::None);
    catalog_flow.set_column_spacing(12);
    catalog_flow.set_row_spacing(12);
    catalog_flow.set_margin_start(16); catalog_flow.set_margin_end(16);
    catalog_flow.set_margin_bottom(16);

    let inst = installed.borrow();
    populate_catalog(&catalog_flow, CATALOG, "", &inst, favorites);
    drop(inst);

    content.append(&catalog_flow);
    scroll.set_child(Some(&content));
    page.append(&scroll);

    // Search connects to all sections
    let cat_flow2 = catalog_flow.clone();
    let sys_flow2 = sys_flow.clone();
    let installed2 = installed.clone();
    let favorites2 = favorites.clone();
    let smart_list2 = smart_list.clone();
    let smart_title2 = smart_title.clone();
    search.connect_search_changed(move |e| {
        let q = e.text().to_string();
        let ql = q.to_lowercase();
        let searching = !q.is_empty();

        // Smart actions
        while let Some(child) = smart_list2.first_child() { smart_list2.remove(&child); }
        if searching {
            let mut count = 0;
            for (kw, icon, name, sub, cmd) in SMART_ACTIONS {
                if kw.contains(ql.as_str()) || name.to_lowercase().contains(ql.as_str()) {
                    smart_list2.append(&make_smart_row(icon, name, sub, cmd));
                    count += 1;
                    if count >= 4 { break; }
                }
            }
        }
        let has_smart = smart_list2.observe_children().n_items() > 0;
        smart_list2.set_visible(has_smart);
        smart_title2.set_visible(has_smart);

        // Filter system apps
        let mut i = 0;
        while let Some(child) = sys_flow2.child_at_index(i) {
            let show = !searching || {
                let name = SYSTEM_APPS.get(i as usize).map(|a| a.1).unwrap_or("");
                name.to_lowercase().contains(&ql)
            };
            child.set_visible(show);
            i += 1;
        }
        // Re-populate catalog
        let inst = installed2.borrow();
        populate_catalog(&cat_flow2, CATALOG, &q, &inst, &favorites2);
    });

    page
}

fn populate_catalog(
    flow: &gtk4::FlowBox,
    apps: &[CatalogApp],
    query: &str,
    installed: &[String],
    favorites: &Rc<RefCell<Vec<String>>>,
) {
    while let Some(child) = flow.first_child() { flow.remove(&child); }
    let q = query.to_lowercase();
    let filtered: Vec<_> = apps.iter().filter(|a| {
        q.is_empty()
            || a.name.to_lowercase().contains(&q)
            || a.description.to_lowercase().contains(&q)
            || a.category.to_lowercase().contains(&q)
    }).collect();

    for app in filtered {
        let is_installed = installed.iter().any(|id| id == app.id);
        flow.insert(&make_catalog_card(app, is_installed, favorites), -1);
    }
}

fn make_catalog_card(
    app: &CatalogApp,
    installed: bool,
    favorites: &Rc<RefCell<Vec<String>>>,
) -> gtk4::FlowBoxChild {
    let child = gtk4::FlowBoxChild::new();
    child.set_focusable(false);

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    card.add_css_class("app-card");
    card.set_size_request(200, -1);

    let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let icon = gtk4::Image::from_icon_name(app.icon);
    icon.set_pixel_size(32);
    top.append(&icon);

    let cat = gtk4::Label::new(Some(app.category));
    cat.add_css_class("cat-pill");
    cat.set_valign(gtk4::Align::Center);
    cat.set_hexpand(true);
    cat.set_halign(gtk4::Align::End);
    top.append(&cat);

    // Star / favorite button
    let exec_for_fav = format!("flatpak run {}", app.id);
    let is_fav = favorites.borrow().contains(&exec_for_fav);
    let star_btn = gtk4::Button::with_label(if is_fav { "★" } else { "☆" });
    star_btn.add_css_class("star-btn");
    let fav_rc = favorites.clone();
    let exec_fav2 = exec_for_fav.clone();
    let star_btn2 = star_btn.clone();
    star_btn.connect_clicked(move |_| {
        let mut favs = fav_rc.borrow_mut();
        if favs.contains(&exec_fav2) {
            favs.retain(|e| e != &exec_fav2);
            star_btn2.set_label("☆");
        } else {
            favs.push(exec_fav2.clone());
            star_btn2.set_label("★");
        }
        save_favorites(&favs);
    });
    top.append(&star_btn);
    card.append(&top);

    let name = gtk4::Label::new(Some(app.name));
    name.add_css_class("app-name");
    name.set_halign(gtk4::Align::Start);
    card.append(&name);

    let desc = gtk4::Label::new(Some(app.description));
    desc.add_css_class("app-desc");
    desc.set_halign(gtk4::Align::Start);
    desc.set_wrap(true);
    desc.set_lines(2);
    card.append(&desc);

    let btn = if installed {
        let b = gtk4::Button::new();
        let hb = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        hb.set_halign(gtk4::Align::Center);
        hb.append(&gtk4::Label::new(Some("Installed ✓")));
        b.set_child(Some(&hb));
        b.add_css_class("installed-btn");
        b.set_sensitive(false);
        b
    } else {
        let b = gtk4::Button::with_label("Install");
        b.add_css_class("suggested-action");
        let id = app.id;
        b.connect_clicked(move |_| install_app(id));
        b
    };
    btn.set_halign(gtk4::Align::Fill);
    card.append(&btn);

    child.set_child(Some(&card));
    child
}

// ── Tab 2: Running ───────────────────────────────────────────────────────────

fn build_running_tab() -> (gtk4::Box, gtk4::ListBox) {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let hint = gtk4::Label::new(Some("Open windows on your desktop"));
    hint.add_css_class("section-title");
    hint.set_halign(gtk4::Align::Start);
    page.append(&hint);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&list)
        .build();
    page.append(&scroll);

    (page, list)
}

fn refresh_running(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() { list.remove(&child); }

    // Speak only the window-management abstraction — never a specific
    // compositor's IPC. Backend (KWin today, ArkaWM later) is chosen inside
    // arka_shell_common::window_service().
    let wm = window_service();
    let windows = wm.list();
    if windows.is_empty() {
        let lbl = gtk4::Label::new(Some("No open windows"));
        lbl.add_css_class("empty-label");
        lbl.set_halign(gtk4::Align::Center);
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&lbl));
        row.set_activatable(false);
        list.append(&row);
        return;
    }

    for win in windows {
        let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row_box.add_css_class("win-row");

        let icon = gtk4::Image::from_icon_name(wm_class_icon(&win.app_id));
        icon.set_pixel_size(20);

        let vb = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        vb.set_hexpand(true);
        let title = gtk4::Label::new(Some(&win.title));
        title.add_css_class("win-title");
        title.set_halign(gtk4::Align::Start);
        title.set_max_width_chars(50);
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let class = gtk4::Label::new(Some(&win.app_id));
        class.add_css_class("win-class");
        class.set_halign(gtk4::Align::Start);
        vb.append(&title);
        vb.append(&class);

        let focus_btn = gtk4::Button::with_label("Focus");
        focus_btn.add_css_class("win-focus-btn");
        let focus_id = win.id.clone();
        focus_btn.connect_clicked(move |_| {
            window_service().focus(&focus_id);
        });

        let close_btn = gtk4::Button::with_label("✕");
        close_btn.add_css_class("win-close-btn");
        let close_id = win.id.clone();
        close_btn.connect_clicked(move |_| {
            window_service().close(&close_id);
        });

        row_box.append(&icon);
        row_box.append(&vb);
        row_box.append(&focus_btn);
        row_box.append(&close_btn);

        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&row_box));
        row.set_activatable(false);
        list.append(&row);
    }
}

fn wm_class_icon(class: &str) -> &'static str {
    let c = class.to_lowercase();
    if c.contains("firefox") || c.contains("browser") { return "web-browser-symbolic"; }
    if c.contains("konsole") || c.contains("terminal") || c.contains("foot") { return "utilities-terminal-symbolic"; }
    if c.contains("dolphin") || c.contains("file") || c.contains("thunar") { return "folder-symbolic"; }
    if c.contains("thunderbird") { return "mail-unread-symbolic"; }
    if c.contains("signal") { return "chat-symbolic"; }
    if c.contains("vlc") { return "multimedia-player-symbolic"; }
    if c.contains("gimp") || c.contains("inkscape") { return "image-x-generic-symbolic"; }
    if c.contains("blender") { return "image-x-generic-symbolic"; }
    if c.contains("obs") { return "video-x-generic-symbolic"; }
    if c.contains("codium") || c.contains("code") { return "utilities-terminal-symbolic"; }
    "application-x-executable-symbolic"
}

// ── Tab 3: Favorites ─────────────────────────────────────────────────────────

fn build_favorites_tab(favorites: &Rc<RefCell<Vec<String>>>) -> (gtk4::Box, gtk4::FlowBox) {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let hint = gtk4::Label::new(Some("Pinned Apps"));
    hint.add_css_class("section-title");
    hint.set_halign(gtk4::Align::Start);
    page.append(&hint);

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let fav_grid = gtk4::FlowBox::new();
    fav_grid.set_max_children_per_line(5);
    fav_grid.set_min_children_per_line(3);
    fav_grid.set_selection_mode(gtk4::SelectionMode::None);
    fav_grid.set_column_spacing(6);
    fav_grid.set_row_spacing(6);
    fav_grid.set_margin_start(14); fav_grid.set_margin_end(14);
    fav_grid.set_margin_top(6); fav_grid.set_margin_bottom(16);

    repopulate_favorites(&fav_grid, &favorites.borrow());

    scroll.set_child(Some(&fav_grid));
    page.append(&scroll);

    (page, fav_grid)
}

fn repopulate_favorites(grid: &gtk4::FlowBox, favorites: &[String]) {
    while let Some(child) = grid.first_child() { grid.remove(&child); }

    // System apps that are favorited
    for (icon, name, cmd) in SYSTEM_APPS {
        if favorites.contains(&cmd.to_string()) {
            grid.insert(&make_fav_button(icon, name, cmd), -1);
        }
    }

    // Catalog apps that are starred (exec = "flatpak run <id>")
    for fav in favorites {
        if fav.starts_with("flatpak run ") {
            let id = &fav["flatpak run ".len()..];
            if let Some(app) = CATALOG.iter().find(|a| a.id == id) {
                let icon = catalog_icon_emoji(app.category);
                grid.insert(&make_fav_button(icon, app.name, fav), -1);
            }
        }
    }

    if grid.observe_children().n_items() == 0 {
        let lbl = gtk4::Label::new(Some("No favorites yet\nStar apps in the Apps tab to pin them here"));
        lbl.add_css_class("empty-label");
        lbl.set_halign(gtk4::Align::Center);
        lbl.set_justify(gtk4::Justification::Center);
        let child = gtk4::FlowBoxChild::new();
        child.set_child(Some(&lbl));
        child.set_focusable(false);
        grid.insert(&child, -1);
    }
}

fn make_fav_button(icon: &str, name: &str, cmd: &str) -> gtk4::FlowBoxChild {
    let child = gtk4::FlowBoxChild::new();
    child.set_focusable(false);

    let btn = gtk4::Button::new();
    btn.add_css_class("fav-app-btn");
    let vb = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vb.set_halign(gtk4::Align::Center);
    let icon_img = gtk4::Image::from_icon_name(icon);
    icon_img.set_pixel_size(24);
    let name_lbl = gtk4::Label::new(Some(name));
    name_lbl.add_css_class("sys-app-name");
    vb.append(&icon_img);
    vb.append(&name_lbl);
    btn.set_child(Some(&vb));
    let c = cmd.to_string();
    btn.connect_clicked(move |_| launch(&c));
    child.set_child(Some(&btn));
    child
}

fn catalog_icon_emoji(category: &str) -> &'static str {
    match category {
        "Privacy"      => "security-high-symbolic",
        "Media"        => "multimedia-player-symbolic",
        "Productivity" => "x-office-document-symbolic",
        "Creative"     => "applications-graphics-symbolic",
        "Developer"    => "utilities-terminal-symbolic",
        _              => "application-x-addon-symbolic",
    }
}

// ── Persistence ──────────────────────────────────────────────────────────────

fn favorites_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".config/arkaos/capsule-favorites.json")
}

fn load_favorites() -> Vec<String> {
    let path = favorites_path();
    let Ok(data) = std::fs::read_to_string(&path) else { return Vec::new() };
    // Parse simple JSON array of strings
    let mut result = Vec::new();
    let mut rest = data.as_str().trim_start_matches([' ', '[', '\n']);
    while let Some(start) = rest.find('"') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('"') {
            let s = rest[..end].to_string();
            if !s.is_empty() { result.push(s); }
            rest = &rest[end + 1..];
        }
    }
    result
}

fn save_favorites(favorites: &[String]) {
    let path = favorites_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = format!("[{}]",
        favorites.iter()
            .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(",")
    );
    let _ = std::fs::write(&path, json);
}

// ── Utils ────────────────────────────────────────────────────────────────────

fn install_app(id: &str) {
    let cmd = format!(
        "flatpak install -y flathub '{}' && echo '✓ Installed!' || echo '✗ Installation failed'; read -p 'Press Enter...'",
        id
    );
    let _ = std::process::Command::new("konsole")
        .args(["-e", "sh", "-c", &cmd])
        .spawn();
}

fn flatpak_installed() -> Vec<String> {
    let out = std::process::Command::new("flatpak")
        .args(["list", "--app", "--columns=application"])
        .output()
        .ok();
    let Some(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn make_smart_row(icon: &str, name: &str, sub: &str, cmd: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let hb = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    hb.set_margin_start(12); hb.set_margin_end(12);
    hb.set_margin_top(10); hb.set_margin_bottom(10);

    let icon_img = gtk4::Image::from_icon_name(icon);
    icon_img.set_pixel_size(20);
    icon_img.set_valign(gtk4::Align::Center);

    let vb = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    vb.set_hexpand(true);
    let title = gtk4::Label::new(Some(name));
    title.add_css_class("win-title");
    title.set_halign(gtk4::Align::Start);
    let subtitle = gtk4::Label::new(Some(sub));
    subtitle.add_css_class("win-class");
    subtitle.set_halign(gtk4::Align::Start);
    vb.append(&title);
    vb.append(&subtitle);

    let arrow = gtk4::Label::new(Some("→"));
    arrow.add_css_class("result-action");

    hb.append(&icon_img);
    hb.append(&vb);
    hb.append(&arrow);
    row.set_child(Some(&hb));

    let c = cmd.to_string();
    row.set_activatable(true);
    let gesture = gtk4::GestureClick::new();
    gesture.connect_released(move |_, _, _, _| { launch(&c); });
    row.add_controller(gesture);

    row
}

fn launch(exec: &str) {
    let parts: Vec<&str> = exec.split_whitespace().collect();
    if parts.is_empty() { return; }
    let _ = std::process::Command::new(parts[0]).args(&parts[1..]).spawn();
}
