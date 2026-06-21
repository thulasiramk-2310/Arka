use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::process::Command;

const APP_ID: &str = "org.arka.settings";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Settings")
        .default_width(720)
        .default_height(680)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .section-title { font-size: 13px; font-weight: 700; color: #4fc3f7; }
    .status-ok     { color: #4ade80; font-size: 12px; }
    .status-warn   { color: #f59e0b; font-size: 12px; }
    .kbd           { font-family: monospace; font-size: 11px; color: #4fc3f7; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("Settings", "ArkaOS")));

    // Sidebar navigation
    let nav = adw::NavigationSplitView::new();

    let sidebar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let sidebar_list = gtk4::ListBox::new();
    sidebar_list.set_selection_mode(gtk4::SelectionMode::Single);
    sidebar_list.add_css_class("navigation-sidebar");

    let pages: &[(&str, &str)] = &[
        ("General",    "preferences-system-symbolic"),
        ("Appearance", "applications-graphics-symbolic"),
        ("Privacy",    "security-high-symbolic"),
        ("Internet",   "network-wireless-symbolic"),
        ("Updates",    "software-update-available-symbolic"),
        ("Advanced",   "utilities-terminal-symbolic"),
    ];

    for (label, icon) in pages {
        let row = gtk4::ListBoxRow::new();
        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        inner.set_margin_start(12);
        inner.set_margin_end(12);
        inner.set_margin_top(10);
        inner.set_margin_bottom(10);
        let img = gtk4::Image::from_icon_name(icon);
        img.set_pixel_size(20);
        let lbl = gtk4::Label::new(Some(label));
        lbl.set_halign(gtk4::Align::Start);
        inner.append(&img);
        inner.append(&lbl);
        row.set_child(Some(&inner));
        sidebar_list.append(&row);
    }

    let sidebar_scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&sidebar_list)
        .build();
    sidebar_box.append(&sidebar_scroll);

    // Content stack
    let stack = gtk4::Stack::new();
    stack.add_named(&page_general(),         Some("General"));
    stack.add_named(&page_appearance(&window), Some("Appearance"));
    stack.add_named(&page_privacy(),    Some("Privacy"));
    stack.add_named(&page_internet(),   Some("Internet"));
    stack.add_named(&page_updates(),    Some("Updates"));
    stack.add_named(&page_advanced(),   Some("Advanced"));

    // Wire sidebar selection → stack
    let stack_ref = stack.clone();
    sidebar_list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            let idx = row.index();
            let name = pages.get(idx as usize).map(|(n, _)| *n).unwrap_or("General");
            stack_ref.set_visible_child_name(name);
        }
    });

    // Select first item by default
    sidebar_list.select_row(sidebar_list.row_at_index(0).as_ref());

    let content_scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&stack)
        .build();

    let sidebar_page = adw::NavigationPage::new(&sidebar_box, "Settings");
    let content_page = adw::NavigationPage::new(&content_scroll, "Content");
    nav.set_sidebar(Some(&sidebar_page));
    nav.set_content(Some(&content_page));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&nav));
    window.set_content(Some(&toolbar));

    // Esc closes
    let ctl = gtk4::EventControllerKey::new();
    let w = window.clone();
    ctl.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape { w.close(); glib::Propagation::Stop }
        else { glib::Propagation::Proceed }
    });
    window.add_controller(ctl);

    window.present();
}

// ── Pages ───────────────────────────────────────────────────────────────────

fn page_general() -> gtk4::Widget {
    let b = page_box();

    let group = adw::PreferencesGroup::new();
    group.set_title("Login");

    let autologin = is_autologin_enabled();
    let al_row = adw::SwitchRow::new();
    al_row.set_title("Automatic Login");
    al_row.set_subtitle("Boot straight to desktop without a password prompt");
    al_row.set_active(autologin);
    al_row.connect_active_notify(|r| {
        set_autologin(r.is_active());
    });
    group.add(&al_row);

    let lock_row = adw::ActionRow::new();
    lock_row.set_title("Lock Screen");
    lock_row.set_subtitle("Coming soon");
    lock_row.set_sensitive(false);
    group.add(&lock_row);

    let about_group = adw::PreferencesGroup::new();
    about_group.set_title("About");

    let ver = std::fs::read_to_string("/etc/arkaos-release")
        .unwrap_or_else(|_| "0.1".into());
    let ver_row = adw::ActionRow::new();
    ver_row.set_title("ArkaOS Version");
    ver_row.set_subtitle(ver.trim());
    about_group.add(&ver_row);

    let host_row = adw::ActionRow::new();
    host_row.set_title("Hostname");
    let hostname = Command::new("hostname").output()
        .ok().and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "arka".into());
    host_row.set_subtitle(hostname.trim());
    about_group.add(&host_row);

    b.append(&group);
    b.append(&about_group);
    b.upcast()
}

fn page_appearance(window: &adw::ApplicationWindow) -> gtk4::Widget {
    let b = page_box();

    let group = adw::PreferencesGroup::new();
    group.set_title("Wallpaper");

    // Read current/custom wallpaper path
    let saved_wp = gtk4::glib::user_config_dir()
        .join("arkaos").join("wallpaper");
    let current_path = std::fs::read_to_string(&saved_wp)
        .ok()
        .unwrap_or_else(|| "/usr/share/arka/wallpapers/default.png".into());

    let wp_row = adw::ActionRow::new();
    wp_row.set_title("Current Wallpaper");
    wp_row.set_subtitle(&current_path);
    let wp_img = gtk4::Image::from_icon_name("image-x-generic-symbolic");
    wp_row.add_prefix(&wp_img);
    group.add(&wp_row);

    let custom_row = adw::ActionRow::new();
    custom_row.set_title("Choose Wallpaper");
    custom_row.set_subtitle("Pick any image from your files");
    let choose_btn = gtk4::Button::with_label("Browse…");
    choose_btn.add_css_class("suggested-action");
    choose_btn.add_css_class("pill");
    choose_btn.set_valign(gtk4::Align::Center);
    custom_row.add_suffix(&choose_btn);
    group.add(&custom_row);

    let reset_row = adw::ActionRow::new();
    reset_row.set_title("Reset to Default");
    reset_row.set_subtitle("Restore the ArkaOS branded wallpaper");
    let reset_btn = gtk4::Button::with_label("Reset");
    reset_btn.add_css_class("pill");
    reset_btn.set_valign(gtk4::Align::Center);
    reset_row.add_suffix(&reset_btn);
    group.add(&reset_row);

    let theme_group = adw::PreferencesGroup::new();
    theme_group.set_title("Theme");

    let dark_row = adw::SwitchRow::new();
    dark_row.set_title("Dark Mode");
    dark_row.set_subtitle("ArkaOS is always dark — privacy is the aesthetic");
    dark_row.set_active(true);
    dark_row.set_sensitive(false);
    theme_group.add(&dark_row);

    // Wire up Browse button
    let wp_row_ref = wp_row.clone();
    let window_weak = window.downgrade();
    choose_btn.connect_clicked(move |_| {
        let dialog = gtk4::FileDialog::new();
        dialog.set_title("Choose Wallpaper");
        let filter = gtk4::FileFilter::new();
        filter.set_name(Some("Images"));
        filter.add_mime_type("image/png");
        filter.add_mime_type("image/jpeg");
        filter.add_mime_type("image/webp");
        filter.add_mime_type("image/gif");
        let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        let wp_row2 = wp_row_ref.clone();
        let parent: Option<gtk4::Window> = window_weak.upgrade().map(|w| w.upcast());
        dialog.open(parent.as_ref(), None::<&gtk4::gio::Cancellable>, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let path_str = path.to_string_lossy().to_string();
                    wp_row2.set_subtitle(&path_str);
                    let conf = gtk4::glib::user_config_dir().join("arkaos");
                    let _ = std::fs::create_dir_all(&conf);
                    let _ = std::fs::write(conf.join("wallpaper"), &path_str);
                    let _ = Command::new("pkill").arg("swaybg").status();
                    let _ = Command::new("swaybg").args(["-i", &path_str, "-m", "fill"]).spawn();
                }
            }
        });
    });

    // Wire up Reset button
    let wp_row_ref2 = wp_row.clone();
    reset_btn.connect_clicked(move |_| {
        let default = "/usr/share/arka/wallpapers/default.png";
        wp_row_ref2.set_subtitle(default);
        let conf = gtk4::glib::user_config_dir().join("arkaos");
        let _ = std::fs::remove_file(conf.join("wallpaper"));
        let _ = Command::new("pkill").arg("swaybg").status();
        let _ = Command::new("swaybg").args(["-i", default, "-m", "fill"]).spawn();
    });

    b.append(&group);
    b.append(&theme_group);
    b.upcast()
}

fn page_privacy() -> gtk4::Widget {
    let b = page_box();

    let group = adw::PreferencesGroup::new();
    group.set_title("Privacy Services");
    group.set_description(Some("These run in the background and protect you automatically."));

    let services = [
        ("DNS Privacy",         "Searches go over encrypted DNS (DoT) to Quad9",     check_dot_active()),
        ("MAC Randomisation",   "Network identity changes on every connection",        check_mac_random()),
        ("Private Hostname",    "Computer shows as 'arka' on all networks",           true),
        ("IPv6 Privacy",        "Temporary addresses prevent long-term tracking",      check_ipv6_priv()),
        ("Browser Sandbox",     "Firefox cannot read your files or passwords",         check_bwrap()),
        ("Immutable System",    "System files are read-only — malware can't persist", true),
    ];

    for (title, subtitle, active) in &services {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(subtitle);
        let status = gtk4::Label::new(Some(if *active { "Active" } else { "Inactive" }));
        status.add_css_class(if *active { "status-ok" } else { "status-warn" });
        row.add_suffix(&status);
        group.add(&row);
    }

    b.append(&group);
    b.upcast()
}

fn page_internet() -> gtk4::Widget {
    let b = page_box();

    let group = adw::PreferencesGroup::new();
    group.set_title("Network");

    let wifi_row = adw::ActionRow::new();
    wifi_row.set_title("WiFi Networks");
    wifi_row.set_subtitle("Open the WiFi picker");
    wifi_row.set_activatable(true);
    wifi_row.connect_activated(|_| {
        let _ = Command::new("arka-wifi").spawn();
    });
    let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
    wifi_row.add_suffix(&chevron);
    group.add(&wifi_row);

    let dns_group = adw::PreferencesGroup::new();
    dns_group.set_title("DNS");

    let dns_row = adw::ActionRow::new();
    dns_row.set_title("DNS-over-TLS");
    dns_row.set_subtitle("Quad9 (9.9.9.9) — privacy-first, malware-blocking");
    let status = gtk4::Label::new(Some(if check_dot_active() { "Active" } else { "Inactive" }));
    status.add_css_class(if check_dot_active() { "status-ok" } else { "status-warn" });
    dns_row.add_suffix(&status);
    dns_group.add(&dns_row);

    b.append(&group);
    b.append(&dns_group);
    b.upcast()
}

fn page_updates() -> gtk4::Widget {
    let b = page_box();

    let group = adw::PreferencesGroup::new();
    group.set_title("System Updates");
    group.set_description(Some("ArkaOS uses bootc for atomic, rollback-safe updates."));

    let ver = std::fs::read_to_string("/etc/arkaos-release")
        .unwrap_or_else(|_| "0.1".into());
    let cur_row = adw::ActionRow::new();
    cur_row.set_title("Current Version");
    cur_row.set_subtitle(ver.trim());
    group.add(&cur_row);

    let check_btn = gtk4::Button::with_label("Check for Updates");
    check_btn.add_css_class("suggested-action");
    check_btn.add_css_class("pill");
    check_btn.set_halign(gtk4::Align::Start);
    check_btn.connect_clicked(|_| run_in_terminal("sudo bootc upgrade --check && read -p 'Press Enter...'"));

    let update_btn = gtk4::Button::with_label("Update Now");
    update_btn.add_css_class("suggested-action");
    update_btn.add_css_class("pill");
    update_btn.set_halign(gtk4::Align::Start);
    update_btn.connect_clicked(|_| run_in_terminal("sudo bootc upgrade && read -p 'Reboot to apply. Press Enter...'"));

    let rollback_btn = gtk4::Button::with_label("Roll Back");
    rollback_btn.add_css_class("destructive-action");
    rollback_btn.add_css_class("pill");
    rollback_btn.set_halign(gtk4::Align::Start);
    rollback_btn.connect_clicked(|_| run_in_terminal("sudo bootc rollback && read -p 'Reboot to apply. Press Enter...'"));

    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_box.set_margin_top(8);
    btn_box.append(&check_btn);
    btn_box.append(&update_btn);
    btn_box.append(&rollback_btn);

    b.append(&group);
    b.append(&btn_box);
    b.upcast()
}

fn page_advanced() -> gtk4::Widget {
    let b = page_box();

    let group = adw::PreferencesGroup::new();
    group.set_title("System");

    let bootc_row = adw::ActionRow::new();
    bootc_row.set_title("bootc status");
    bootc_row.set_activatable(true);
    bootc_row.connect_activated(|_| run_in_terminal("bootc status; read -p 'Press Enter...'"));
    let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
    bootc_row.add_suffix(&chevron);
    group.add(&bootc_row);

    let composefs_row = adw::ActionRow::new();
    composefs_row.set_title("Immutable Filesystem");
    composefs_row.set_subtitle("composefs — read-only overlay via ostree");
    let composefs_icon = gtk4::Image::from_icon_name("object-locked-symbolic");
    composefs_row.add_suffix(&composefs_icon);
    group.add(&composefs_row);

    let tpm_row = adw::ActionRow::new();
    tpm_row.set_title("TPM / Measured Boot");
    tpm_row.set_activatable(true);
    tpm_row.connect_activated(|_| run_in_terminal("systemd-analyze pcrs 2>/dev/null || echo 'No TPM'; read -p 'Press Enter...'"));
    let chevron2 = gtk4::Image::from_icon_name("go-next-symbolic");
    tpm_row.add_suffix(&chevron2);
    group.add(&tpm_row);

    let logs_group = adw::PreferencesGroup::new();
    logs_group.set_title("Logs");

    let journal_row = adw::ActionRow::new();
    journal_row.set_title("System Journal");
    journal_row.set_activatable(true);
    journal_row.connect_activated(|_| run_in_terminal("journalctl -n 50 --no-pager; read -p 'Press Enter...'"));
    let chevron3 = gtk4::Image::from_icon_name("go-next-symbolic");
    journal_row.add_suffix(&chevron3);
    logs_group.add(&journal_row);

    let arkad_row = adw::ActionRow::new();
    arkad_row.set_title("arkad Logs");
    arkad_row.set_activatable(true);
    arkad_row.connect_activated(|_| run_in_terminal("journalctl -u arkad -n 30 --no-pager; read -p 'Press Enter...'"));
    let chevron4 = gtk4::Image::from_icon_name("go-next-symbolic");
    arkad_row.add_suffix(&chevron4);
    logs_group.add(&arkad_row);

    b.append(&group);
    b.append(&logs_group);
    b.upcast()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn page_box() -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, 20);
    b.set_margin_start(20);
    b.set_margin_end(20);
    b.set_margin_top(20);
    b.set_margin_bottom(20);
    b
}

fn run_in_terminal(cmd: &str) {
    let _ = Command::new("foot").args(["-e", "sh", "-c", cmd]).spawn();
}

fn is_autologin_enabled() -> bool {
    std::path::Path::new("/etc/systemd/system/getty@tty1.service.d/autologin.conf").exists()
}

fn set_autologin(enable: bool) {
    if enable {
        // Read the current username from /etc/passwd for the first non-root user
        let user = current_username();
        let _ = std::fs::create_dir_all("/etc/systemd/system/getty@tty1.service.d");
        let _ = std::fs::write(
            "/etc/systemd/system/getty@tty1.service.d/autologin.conf",
            format!("[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin {} --noclear %I linux\n", user),
        );
    } else {
        let _ = std::fs::remove_file("/etc/systemd/system/getty@tty1.service.d/autologin.conf");
    }
    let _ = Command::new("systemctl").args(["daemon-reload"]).output();
}

fn current_username() -> String {
    std::env::var("USER").unwrap_or_else(|_| "ram".into())
}

fn check_dot_active() -> bool {
    std::path::Path::new("/etc/systemd/resolved.conf.d/99-arkad-dot.conf").exists()
}

fn check_mac_random() -> bool {
    std::path::Path::new("/etc/NetworkManager/conf.d/00-arkaos-mac-random.conf").exists()
}

fn check_ipv6_priv() -> bool {
    std::fs::read_to_string("/proc/sys/net/ipv6/conf/all/use_tempaddr")
        .ok().map(|s| s.trim() == "2").unwrap_or(false)
}

fn check_bwrap() -> bool {
    // firefox symlink points to sandbox wrapper
    std::fs::read_link("/usr/bin/firefox")
        .ok().map(|p| p.to_string_lossy().contains("sandbox")).unwrap_or(false)
}
