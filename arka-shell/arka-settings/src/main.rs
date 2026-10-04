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
    // Follow the desktop look (Light/Dark) instead of forcing dark.
    arka_shell_common::theme::follow_look();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Settings")
        .default_width(860)
        .default_height(620)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .section-title { font-size: 13px; font-weight: 700; color: @accent; }
    .status-ok     { color: @accent; font-size: 12px; font-weight: 600; }
    .status-warn   { color: @warn; font-size: 12px; font-weight: 600; }
    .kbd           { font-family: 'JetBrains Mono', monospace; font-size: 11px; color: @text_lo; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_show_title(false);

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
    arka_shell_common::theme::preview_shot(&window);
}

// ── Pages ───────────────────────────────────────────────────────────────────

fn page_general() -> gtk4::Widget {
    let b = page_box("General", "Signing in, locking the screen, and this computer.");

    let group = adw::PreferencesGroup::new();
    group.set_title("Login");

    let al_row = adw::ActionRow::new();
    al_row.set_title("Login screen and automatic sign-in");
    al_row.set_subtitle("Opens Plasma's login screen settings. Changing them asks for an administrator password.");
    al_row.set_activatable(true);
    al_row.connect_activated(|_| { let _ = Command::new("systemsettings").arg("kcm_sddm").spawn(); });
    group.add(&al_row);
    let lock_row = adw::ActionRow::new();
    lock_row.set_title("Screen locking");
    lock_row.set_activatable(true);
    lock_row.connect_activated(|_| { let _ = Command::new("systemsettings").arg("kcm_screenlocker").spawn(); });
    group.add(&lock_row);

    let about_group = adw::PreferencesGroup::new();
    about_group.set_title("About");

    let ver = std::fs::read_to_string("/usr/lib/arkaos-release")
        .unwrap_or_else(|_| "0.1".into());
    let ver_row = adw::ActionRow::new();
    ver_row.set_title("ArkaOS version");
    ver_row.set_subtitle(ver.trim());
    about_group.add(&ver_row);

    let host_row = adw::ActionRow::new();
    host_row.set_title("Network name");
    let hostname = Command::new("hostname").output()
        .ok().and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "arka".into());
    host_row.set_subtitle(hostname.trim());
    about_group.add(&host_row);

    b.append(&group);
    b.append(&about_group);
    b.upcast()
}

fn current_look() -> &'static str {
    let p = gtk4::glib::user_config_dir().join("arka-look");
    if std::fs::read_to_string(p).map(|s| s.trim() == "light").unwrap_or(false) { "light" } else { "dark" }
}

fn page_appearance(window: &adw::ApplicationWindow) -> gtk4::Widget {
    let b = page_box("Appearance", "Light or dark, and your wallpaper. Everything here stays yours to change.");

    let group = adw::PreferencesGroup::new();
    group.set_title("Wallpaper");

    // Read current/custom wallpaper path
    let saved_wp = gtk4::glib::user_config_dir()
        .join("arkaos").join("wallpaper");
    let current_path = std::fs::read_to_string(&saved_wp)
        .ok()
        .unwrap_or_else(|| format!("/usr/share/arka/wallpapers/default-{}.png", current_look()));

    let wp_row = adw::ActionRow::new();
    wp_row.set_title("Current wallpaper");
    wp_row.set_subtitle(&current_path);
    let wp_img = gtk4::Image::from_icon_name("image-x-generic-symbolic");
    wp_row.add_prefix(&wp_img);
    group.add(&wp_row);

    let custom_row = adw::ActionRow::new();
    custom_row.set_title("Choose a wallpaper");
    custom_row.set_subtitle("Pick any image from your files.");
    let choose_btn = gtk4::Button::with_label("Browse…");
    choose_btn.add_css_class("suggested-action");
    choose_btn.add_css_class("pill");
    choose_btn.set_valign(gtk4::Align::Center);
    custom_row.add_suffix(&choose_btn);
    group.add(&custom_row);

    let reset_row = adw::ActionRow::new();
    reset_row.set_title("Use the ArkaOS wallpaper");
    reset_row.set_subtitle("The one that matches your look.");
    let reset_btn = gtk4::Button::with_label("Reset");
    reset_btn.add_css_class("pill");
    reset_btn.set_valign(gtk4::Align::Center);
    reset_row.add_suffix(&reset_btn);
    group.add(&reset_row);

    let theme_group = adw::PreferencesGroup::new();
    theme_group.set_title("Look");
    theme_group.set_description(Some(
        "Applies to the desktop and ArkaOS apps. The login screen is shared by everyone on this \
         computer and keeps its look until an administrator changes it.",
    ));
    let look_row = adw::ActionRow::new();
    look_row.set_title("Light or dark");
    let look_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    look_box.add_css_class("linked");
    look_box.set_valign(gtk4::Align::Center);
    let light_btn = gtk4::ToggleButton::with_label("Light");
    let dark_btn = gtk4::ToggleButton::with_label("Dark");
    dark_btn.set_group(Some(&light_btn));
    if current_look() == "light" { light_btn.set_active(true) } else { dark_btn.set_active(true) }
    look_box.append(&light_btn);
    look_box.append(&dark_btn);
    look_row.add_suffix(&look_box);
    theme_group.add(&look_row);
    for (btn, look) in [(&light_btn, "light"), (&dark_btn, "dark")] {
        let wp = wp_row.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() || current_look() == look { return; }
            let wp = wp.clone();
            glib::spawn_future_local(async move {
                let ok = gtk4::gio::spawn_blocking(move || {
                    Command::new("/usr/libexec/arka-apply-look").arg(look).status().map(|s| s.success()).unwrap_or(false)
                }).await.unwrap_or(false);
                if !ok { wp.set_subtitle("Couldn't switch the look. Try System Settings > Colors & Themes."); }
            });
        });
    }

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
                    apply_wallpaper(path_str, wp_row2.clone());
                }
            }
        });
    });

    // Wire up Reset button
    let wp_row_ref2 = wp_row.clone();
    reset_btn.connect_clicked(move |_| {
        let default = format!("/usr/share/arka/wallpapers/default-{}.png", current_look());
        apply_wallpaper(default, wp_row_ref2.clone());
    });

    b.append(&theme_group);
    b.append(&group);
    b.upcast()
}

fn page_privacy() -> gtk4::Widget {
    let b = page_box("Privacy", "What ArkaOS has set up, and what your apps may use.");
    let group = adw::PreferencesGroup::new();
    for (title, subtitle, program) in [
        ("Privacy dashboard", "Which privacy settings are in place right now, as last checked.", "arka-dashboard"),
        ("App permissions", "What apps installed with Capsule are allowed to use.", "arka-perms"),
    ] {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(subtitle);
        row.set_activatable(true);
        row.connect_activated(move |_| { let _ = Command::new(program).spawn(); });
        row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        group.add(&row);
    }
    b.append(&group);
    b.upcast()
}

fn page_internet() -> gtk4::Widget {
    let b = page_box("Internet", "Wi-Fi and how website names are looked up.");

    let group = adw::PreferencesGroup::new();
    group.set_title("Network");

    let wifi_row = adw::ActionRow::new();
    wifi_row.set_title("Wi-Fi networks");
    wifi_row.set_subtitle("Choose a network and sign in.");
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
    dns_row.set_title("Encrypted DNS");
    dns_row.set_subtitle("ArkaOS sets system DNS to require encryption (DNS over TLS, Quad9). Its current state is in the Privacy dashboard.");
    dns_row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    dns_row.set_activatable(true);
    dns_row.connect_activated(|_| { let _ = Command::new("arka-dashboard").spawn(); });
    dns_group.add(&dns_row);

    b.append(&group);
    b.append(&dns_group);
    b.upcast()
}

fn page_updates() -> gtk4::Widget {
    let b = page_box(
        "Updates",
        "New versions download next to the one you're using and take over only when you restart. \
         The previous version is kept so you can go back.",
    );
    let group = adw::PreferencesGroup::new();
    let ver = std::fs::read_to_string("/usr/lib/arkaos-release").unwrap_or_default();
    let cur_row = adw::ActionRow::new();
    cur_row.set_title("ArkaOS version");
    cur_row.set_subtitle(if ver.trim().is_empty() { "Shown in the Updates app" } else { ver.trim() });
    group.add(&cur_row);
    let open_row = adw::ActionRow::new();
    open_row.set_title("Check, download, or go back");
    open_row.set_subtitle("Opens the Updates app.");
    open_row.set_activatable(true);
    open_row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    open_row.connect_activated(|_| { let _ = Command::new("arka-update").spawn(); });
    group.add(&open_row);
    b.append(&group);
    b.upcast()
}

fn page_advanced() -> gtk4::Widget {
    let b = page_box("Advanced", "Details for troubleshooting. These open a terminal with read-only commands.");

    let group = adw::PreferencesGroup::new();
    group.set_title("System");

    let bootc_row = adw::ActionRow::new();
    bootc_row.set_title("System image status");
    bootc_row.set_subtitle("bootc status");
    bootc_row.set_activatable(true);
    bootc_row.connect_activated(|_| run_in_terminal("bootc status; read -p 'Press Enter...'"));
    let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
    bootc_row.add_suffix(&chevron);
    group.add(&bootc_row);

    let composefs_row = adw::ActionRow::new();
    composefs_row.set_title("Read-only system image");
    composefs_row.set_subtitle("The bootc system image is read-only; user data and system configuration remain writable");
    let composefs_icon = gtk4::Image::from_icon_name("object-locked-symbolic");
    composefs_row.add_suffix(&composefs_icon);
    group.add(&composefs_row);

    let tpm_row = adw::ActionRow::new();
    tpm_row.set_title("TPM measurements");
    tpm_row.set_subtitle("systemd-analyze pcrs");
    tpm_row.set_activatable(true);
    tpm_row.connect_activated(|_| run_in_terminal("systemd-analyze pcrs 2>/dev/null || echo 'No TPM'; read -p 'Press Enter...'"));
    let chevron2 = gtk4::Image::from_icon_name("go-next-symbolic");
    tpm_row.add_suffix(&chevron2);
    group.add(&tpm_row);

    let logs_group = adw::PreferencesGroup::new();
    logs_group.set_title("Logs");

    let journal_row = adw::ActionRow::new();
    journal_row.set_title("System log");
    journal_row.set_subtitle("The last 50 entries");
    journal_row.set_activatable(true);
    journal_row.connect_activated(|_| run_in_terminal("journalctl -n 50 --no-pager; read -p 'Press Enter...'"));
    let chevron3 = gtk4::Image::from_icon_name("go-next-symbolic");
    journal_row.add_suffix(&chevron3);
    logs_group.add(&journal_row);

    let arkad_row = adw::ActionRow::new();
    arkad_row.set_title("Privacy service log");
    arkad_row.set_subtitle("arkad, the last 30 entries");
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

fn page_box(title: &str, sub: &str) -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, 20);
    b.set_margin_start(32);
    b.set_margin_end(32);
    b.set_margin_top(24);
    b.set_margin_bottom(24);
    b.append(&arka_shell_common::theme::page_header(title, sub));
    b
}

fn run_in_terminal(cmd: &str) {
    let _ = Command::new("konsole").args(["-e", "sh", "-c", cmd]).spawn();
}

fn apply_wallpaper(path: String, row: adw::ActionRow) {
    glib::spawn_future_local(async move {
        let selected = path.clone();
        let result = gtk4::gio::spawn_blocking(move || {
            Command::new("timeout").args(["15", "plasma-apply-wallpaperimage", &selected]).output()
        }).await;
        if matches!(result, Ok(Ok(ref out)) if out.status.success()) {
            let conf = gtk4::glib::user_config_dir().join("arkaos");
            let _ = std::fs::create_dir_all(&conf);
            let _ = std::fs::write(conf.join("wallpaper"), &path);
            row.set_subtitle(&path);
        } else {
            row.set_subtitle("Could not apply wallpaper. Try Plasma Desktop Settings.");
        }
    });
}
