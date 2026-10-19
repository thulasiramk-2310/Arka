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
        ("Developer tools", "applications-development-symbolic"),
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
    stack.add_named(&page_developer(),  Some("Developer tools"));
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
                if !ok {
                    wp.set_subtitle("Couldn't switch the look. Try System Settings > Colors & Themes.");
                    return;
                }
                // Show the wallpaper that is now in use (unless the user picked their own).
                let custom = std::fs::read_to_string(gtk4::glib::user_config_dir().join("arkaos").join("wallpaper")).unwrap_or_default();
                if custom.is_empty() || custom.starts_with("/usr/share/arka/wallpapers/default") {
                    wp.set_subtitle(&format!("/usr/share/arka/wallpapers/default-{look}.png"));
                }
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

// The VARA Hub runs as this user's systemd service (vara-hub.service), off
// until switched on here, and listens on this computer only.
const HUB_URL: &str = "http://127.0.0.1:8737";

fn hub_dir(sub: &str) -> std::path::PathBuf {
    glib::user_data_dir().join("vara-hub").join(sub)
}

// The commit name and email: VARA's own setting first, Git's otherwise.
fn current_identity() -> (String, String) {
    let file = glib::user_config_dir().join("vara").join("identity");
    let (mut name, mut email) = (String::new(), String::new());
    for line in std::fs::read_to_string(file).unwrap_or_default().lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "name" => name = v.trim().to_string(),
                "email" => email = v.trim().to_string(),
                _ => {}
            }
        }
    }
    let git = |key: &str| {
        Command::new("git").args(["config", "--global", "--get", key]).output()
            .ok().filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    };
    if name.is_empty() { name = git("user.name"); }
    if email.is_empty() { email = git("user.email"); }
    (name, email)
}

fn run_ok(program: &str, args: &[&str]) -> bool {
    Command::new(program).args(args).current_dir(glib::home_dir())
        .output().map(|o| o.status.success()).unwrap_or(false)
}

fn page_developer() -> gtk4::Widget {
    let b = page_box("Developer tools", "Version control for writing code. Everything here stays on this computer.");

    // Name and email for commits, written to VARA's and Git's own settings.
    let id_group = adw::PreferencesGroup::new();
    id_group.set_title("Name and email for commits");
    id_group.set_description(Some("VARA and Git put these on every commit you make."));
    let (name, email) = current_identity();
    let name_row = adw::EntryRow::new();
    name_row.set_title("Name");
    name_row.set_text(&name);
    let email_row = adw::EntryRow::new();
    email_row.set_title("Email");
    email_row.set_text(&email);
    let save_row = adw::ActionRow::new();
    save_row.set_title("Save for VARA and Git");
    let save = gtk4::Button::with_label("Save");
    save.set_valign(gtk4::Align::Center);
    save_row.add_suffix(&save);
    {
        let (n, e, row) = (name_row.clone(), email_row.clone(), save_row.clone());
        save.connect_clicked(move |btn| {
            let (name, email) = (n.text().trim().to_string(), e.text().trim().to_string());
            if !arka_shell_common::commit_identity::valid_name(&name) {
                row.set_subtitle("Enter the name to put on your commits.");
                return;
            }
            if !arka_shell_common::commit_identity::valid_email(&email) {
                row.set_subtitle("Enter an email like you@example.com.");
                return;
            }
            btn.set_sensitive(false);
            let (row, btn) = (row.clone(), btn.clone());
            glib::spawn_future_local(async move {
                let done = gtk4::gio::spawn_blocking(move || {
                    let vara = run_ok("vara", &["identity", "set", "--default", "--name", &name, "--email", &email]);
                    let git = run_ok("git", &["config", "--global", "user.name", &name])
                        && run_ok("git", &["config", "--global", "user.email", &email]);
                    (vara, git)
                }).await;
                row.set_subtitle(match done {
                    Ok((true, true)) => "Saved for VARA and Git.",
                    Ok((true, false)) => "Saved for VARA. Git's setting could not be changed.",
                    Ok((false, true)) => "Saved for Git. VARA's setting could not be changed.",
                    _ => "Could not save. Nothing was changed.",
                });
                btn.set_sensitive(true);
            });
        });
    }
    id_group.add(&name_row);
    id_group.add(&email_row);
    id_group.add(&save_row);

    // The local VARA Hub.
    let hub_group = adw::PreferencesGroup::new();
    hub_group.set_title("VARA Hub on this computer");
    hub_group.set_description(Some(
        "A web view of the repositories in your VARA folder. Only this computer can reach it.",
    ));
    let hub_row = adw::SwitchRow::new();
    hub_row.set_title("Run the VARA Hub");
    hub_row.set_subtitle(HUB_URL);
    hub_row.set_active(run_ok("systemctl", &["--user", "is-enabled", "--quiet", "vara-hub.service"]));
    let pass_row = adw::PasswordEntryRow::new();
    pass_row.set_title("Choose a Hub password (8 characters or more), then press Enter");
    pass_row.set_show_apply_button(true);
    pass_row.set_visible(false);
    let open_row = adw::ActionRow::new();
    open_row.set_title("Open the Hub");
    open_row.set_subtitle("Sign in with your username and Hub password.");
    open_row.set_activatable(true);
    open_row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    open_row.set_sensitive(hub_row.is_active());
    open_row.connect_activated(|_| { let _ = Command::new("xdg-open").arg(HUB_URL).spawn(); });

    // Start or stop the service; the switch shows the result, not the wish.
    let set_hub: std::rc::Rc<dyn Fn(bool)> = {
        let (hub_row, open_row) = (hub_row.clone(), open_row.clone());
        std::rc::Rc::new(move |on: bool| {
            let (hub_row, open_row) = (hub_row.clone(), open_row.clone());
            glib::spawn_future_local(async move {
                let ok = gtk4::gio::spawn_blocking(move || {
                    let verb = if on { "enable" } else { "disable" };
                    run_ok("systemctl", &["--user", verb, "--now", "vara-hub.service"])
                }).await.unwrap_or(false);
                let now = run_ok("systemctl", &["--user", "is-active", "--quiet", "vara-hub.service"]);
                hub_row.set_active(now);
                open_row.set_sensitive(now);
                hub_row.set_subtitle(if ok { HUB_URL } else if on { "The Hub could not start." } else { "The Hub could not stop." });
            });
        })
    };
    {
        let (pass_row, set_hub) = (pass_row.clone(), set_hub.clone());
        hub_row.connect_active_notify(move |row| {
            let has_account = std::fs::read_dir(hub_dir("accounts")).map(|mut d| d.next().is_some()).unwrap_or(false);
            let enabled = run_ok("systemctl", &["--user", "is-enabled", "--quiet", "vara-hub.service"]);
            if row.is_active() == enabled {
                return; // a state change made by set_hub itself
            }
            if row.is_active() && !has_account {
                // First use: the Hub needs an account before it starts.
                row.set_active(false);
                pass_row.set_visible(true);
                pass_row.grab_focus();
                return;
            }
            set_hub(row.is_active());
        });
    }
    {
        let (hub_row, set_hub) = (hub_row.clone(), set_hub.clone());
        pass_row.connect_apply(move |entry| {
            let pass = entry.text().to_string();
            if pass.chars().count() < 8 {
                entry.set_title("Use at least 8 characters, then press Enter");
                return;
            }
            entry.set_sensitive(false);
            let (entry, hub_row, set_hub) = (entry.clone(), hub_row.clone(), set_hub.clone());
            glib::spawn_future_local(async move {
                let created = gtk4::gio::spawn_blocking(move || {
                    for sub in ["accounts", "policy", "meta"] {
                        let _ = std::fs::create_dir_all(hub_dir(sub));
                    }
                    let _ = std::fs::create_dir_all(glib::home_dir().join("VARA"));
                    let (a, p) = (hub_dir("accounts"), hub_dir("policy"));
                    run_ok("vara", &["account", "create", "--accounts", &a.to_string_lossy(),
                        "--policy", &p.to_string_lossy(), "--username", &glib::user_name().to_string_lossy(),
                        "--password", &pass])
                }).await.unwrap_or(false);
                entry.set_text("");
                entry.set_sensitive(true);
                if created {
                    entry.set_visible(false);
                    set_hub(true);
                } else {
                    hub_row.set_subtitle("Could not create the Hub account.");
                }
            });
        });
    }
    hub_group.add(&hub_row);
    hub_group.add(&pass_row);
    hub_group.add(&open_row);

    b.append(&id_group);
    b.append(&hub_group);
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
