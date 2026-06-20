use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::process::Command;

const APP_ID: &str = "org.arka.perms";

// Flatpak permission categories we surface to the user
#[derive(Clone)]
struct Perm {
    label:   &'static str,
    desc:    &'static str,
    icon:    &'static str,
    // The flatpak override flag to add when ALLOWED (deny = --no-*)
    flag:    &'static str,
    // How to detect if currently allowed in `flatpak info --show-permissions`
    detect:  &'static str,
}

const PERMS: &[Perm] = &[
    Perm {
        label: "Network Access",
        desc:  "Connect to the internet",
        icon:  "network-wireless-symbolic",
        flag:  "--share=network",
        detect: "network",
    },
    Perm {
        label: "Microphone",
        desc:  "Record audio from your microphone",
        icon:  "audio-input-microphone-symbolic",
        flag:  "--device=all",
        detect: "all-devices",
    },
    Perm {
        label: "Camera",
        desc:  "Access your webcam",
        icon:  "camera-web-symbolic",
        flag:  "--device=dri",
        detect: "dri",
    },
    Perm {
        label: "Home Folder",
        desc:  "Read and write your personal files",
        icon:  "user-home-symbolic",
        flag:  "--filesystem=home",
        detect: "home",
    },
    Perm {
        label: "Downloads Folder",
        desc:  "Read and write ~/Downloads only",
        icon:  "folder-download-symbolic",
        flag:  "--filesystem=xdg-download",
        detect: "xdg-download",
    },
    Perm {
        label: "Wayland Display",
        desc:  "Show windows on your screen",
        icon:  "video-display-symbolic",
        flag:  "--socket=wayland",
        detect: "wayland",
    },
    Perm {
        label: "Audio Playback",
        desc:  "Play sound through your speakers",
        icon:  "audio-volume-high-symbolic",
        flag:  "--socket=pulseaudio",
        detect: "pulseaudio",
    },
    Perm {
        label: "USB Devices",
        desc:  "Access USB drives and peripherals",
        icon:  "drive-removable-media-symbolic",
        flag:  "--device=usb",
        detect: "usb",
    },
];

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("App Permissions")
        .default_width(620)
        .default_height(680)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .perm-app-name { font-size: 15px; font-weight: 700; color: #d8e8f8; }
    .perm-app-id   { font-size: 11px; color: #3a5a78; }
    .danger-row    { color: #f28b35; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("App Permissions", "ArkaOS")));

    // Main content stack: app list → permission detail
    let nav = adw::NavigationView::new();
    nav.set_animate_transitions(true);

    // ── Page 1: installed app list ──────────────────────────────────────────
    let app_list_page = adw::NavigationPage::new(
        &build_app_list_page(&nav),
        "Apps",
    );
    nav.push(&app_list_page);

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

fn build_app_list_page(nav: &adw::NavigationView) -> gtk4::Widget {
    let apps = list_flatpak_apps();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    if apps.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_icon_name(Some("application-x-executable-symbolic"));
        empty.set_title("No Sandboxed Apps");
        empty.set_description(Some("Install apps via Capsule to manage their permissions here."));
        content.append(&empty);
        return content.upcast();
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Installed Apps");
    group.set_description(Some("Tap an app to review and change what it can access."));
    group.set_margin_start(16);
    group.set_margin_end(16);
    group.set_margin_top(16);
    group.set_margin_bottom(16);

    for (id, name) in &apps {
        let row = adw::ActionRow::new();
        row.set_title(name);
        row.set_subtitle(id);
        row.set_activatable(true);

        let icon = gtk4::Image::from_icon_name("application-x-executable-symbolic");
        icon.set_pixel_size(32);
        row.add_prefix(&icon);

        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        row.add_suffix(&chevron);

        let nav_ref = nav.clone();
        let id_owned = id.clone();
        let name_owned = name.clone();
        row.connect_activated(move |_| {
            let perm_page = build_perm_page(&id_owned, &name_owned);
            let nav_page = adw::NavigationPage::new(&perm_page, &name_owned);
            nav_ref.push(&nav_page);
        });

        group.add(&row);
    }

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    inner.append(&group);
    scroll.set_child(Some(&inner));
    content.append(&scroll);
    content.upcast()
}

fn build_perm_page(app_id: &str, app_name: &str) -> gtk4::Widget {
    let current_perms = read_app_perms(app_id);
    let overrides     = read_app_overrides(app_id);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    // App header card
    let header_group = adw::PreferencesGroup::new();
    header_group.set_margin_start(16);
    header_group.set_margin_end(16);
    header_group.set_margin_top(16);
    header_group.set_margin_bottom(4);

    let header_row = adw::ActionRow::new();
    header_row.set_title(app_name);
    header_row.set_subtitle(app_id);
    let app_icon = gtk4::Image::from_icon_name("application-x-executable-symbolic");
    app_icon.set_pixel_size(40);
    header_row.add_prefix(&app_icon);
    header_group.add(&header_row);

    // Reset button
    let reset_btn = gtk4::Button::with_label("Reset to Defaults");
    reset_btn.add_css_class("destructive-action");
    reset_btn.add_css_class("pill");
    reset_btn.set_halign(gtk4::Align::Center);
    reset_btn.set_margin_top(8);
    reset_btn.set_margin_bottom(4);
    let aid = app_id.to_string();
    reset_btn.connect_clicked(move |_| {
        let _ = Command::new("flatpak")
            .args(["override", "--user", "--reset", &aid])
            .output();
    });

    // Permissions group
    let perm_group = adw::PreferencesGroup::new();
    perm_group.set_title("Permissions");
    perm_group.set_margin_start(16);
    perm_group.set_margin_end(16);
    perm_group.set_margin_top(8);
    perm_group.set_margin_bottom(8);

    for perm in PERMS {
        let row = adw::SwitchRow::new();
        row.set_title(perm.label);
        row.set_subtitle(perm.desc);

        let prefix_icon = gtk4::Image::from_icon_name(perm.icon);
        prefix_icon.set_pixel_size(20);
        row.add_prefix(&prefix_icon);

        // Determine current state: overrides take priority over manifest perms
        let is_on = is_perm_active(perm, &current_perms, &overrides);
        row.set_active(is_on);

        let aid = app_id.to_string();
        let flag = perm.flag;
        row.connect_active_notify(move |r| {
            apply_perm_override(&aid, flag, r.is_active());
        });

        perm_group.add(&row);
    }

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    inner.append(&header_group);
    inner.append(&perm_group);
    inner.append(&reset_btn);
    scroll.set_child(Some(&inner));
    content.append(&scroll);
    content.upcast()
}

// ── flatpak helpers ─────────────────────────────────────────────────────────

fn list_flatpak_apps() -> Vec<(String, String)> {
    let out = Command::new("flatpak")
        .args(["list", "--app", "--columns=application,name"])
        .output()
        .ok();
    let Some(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut parts = l.splitn(2, '\t');
            let id   = parts.next()?.trim().to_string();
            let name = parts.next().unwrap_or(&id).trim().to_string();
            if id.is_empty() { return None; }
            Some((id, name))
        })
        .collect()
}

fn read_app_perms(app_id: &str) -> String {
    Command::new("flatpak")
        .args(["info", "--show-permissions", app_id])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

fn read_app_overrides(app_id: &str) -> String {
    // flatpak override --user --show  (shows user-level overrides)
    Command::new("flatpak")
        .args(["override", "--user", "--show", app_id])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

fn is_perm_active(perm: &Perm, manifest: &str, overrides: &str) -> bool {
    // If there's a user override that explicitly denies, that wins
    let deny_flag = format!("no{}", perm.detect.replace('-', ""));
    if overrides.contains(&deny_flag) { return false; }
    // If there's a user override that explicitly allows, that wins
    if overrides.contains(perm.detect) { return true; }
    // Fall back to manifest permissions
    manifest.contains(perm.detect)
}

fn apply_perm_override(app_id: &str, flag: &str, allow: bool) {
    if allow {
        let _ = Command::new("flatpak")
            .args(["override", "--user", flag, app_id])
            .output();
    } else {
        // Convert --share=network → --no-share=network etc.
        let deny_flag = flag
            .replace("--share=", "--no-share=")
            .replace("--device=", "--no-device=")
            .replace("--filesystem=", "--no-filesystem=")
            .replace("--socket=", "--no-socket=");
        let _ = Command::new("flatpak")
            .args(["override", "--user", &deny_flag, app_id])
            .output();
    }
}
