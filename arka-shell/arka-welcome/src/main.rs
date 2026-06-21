use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::path::PathBuf;

const APP_ID: &str = "org.arka.welcome";

fn marker_path() -> PathBuf {
    let mut p = glib::user_data_dir();
    p.push("arkaos");
    p.push("welcome-shown");
    p
}

fn mark_shown() {
    let p = marker_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&p, "1");
}

fn already_shown() -> bool {
    marker_path().exists()
}

fn spawn(cmd: &str) {
    let _ = std::process::Command::new(cmd).spawn();
}

fn main() {
    if already_shown() {
        return;
    }

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        "
        .welcome-title {
            font-size: 36px;
            font-weight: bold;
            color: #c8d8f0;
        }
        .welcome-sub {
            font-size: 16px;
            color: #8aaccc;
            letter-spacing: 2px;
        }
        .arka-brand {
            font-size: 52px;
            font-weight: bold;
            color: #c8d8f0;
        }
        .feature-check {
            font-size: 15px;
            color: #6fcf8a;
        }
        .feature-label {
            font-size: 15px;
            color: #c0d4e8;
        }
        .action-btn {
            min-width: 130px;
            min-height: 56px;
            font-size: 13px;
        }
        .get-started {
            font-size: 16px;
            font-weight: bold;
            min-height: 52px;
            border-radius: 26px;
        }
        .dark-bg {
            background-color: #070a10;
        }
        "
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Welcome to ArkaOS")
        .default_width(680)
        .default_height(640)
        .resizable(false)
        .build();

    // Root vertical box
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("dark-bg");

    // ── Brand header ──────────────────────────────────────────────────
    let brand_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    brand_box.set_margin_top(48);
    brand_box.set_margin_bottom(24);
    brand_box.set_halign(gtk4::Align::Center);

    let brand_lbl = gtk4::Label::new(Some("▲  ARKA"));
    brand_lbl.add_css_class("arka-brand");

    let title_lbl = gtk4::Label::new(Some("Welcome to ArkaOS"));
    title_lbl.add_css_class("welcome-title");

    let sub_lbl = gtk4::Label::new(Some("YOUR COMPUTER IS YOURS"));
    sub_lbl.add_css_class("welcome-sub");

    brand_box.append(&brand_lbl);
    brand_box.append(&title_lbl);
    brand_box.append(&sub_lbl);

    // ── Privacy features ──────────────────────────────────────────────
    let features: &[&str] = &[
        "Searches are Private",
        "Browser is Protected",
        "Your Device Can't Be Tracked",
        "Your Identity Stays Hidden on Networks",
    ];

    let feat_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    feat_box.set_halign(gtk4::Align::Center);
    feat_box.set_margin_bottom(32);

    for feat in features {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.set_halign(gtk4::Align::Center);

        let check = gtk4::Label::new(Some("✓"));
        check.add_css_class("feature-check");

        let lbl = gtk4::Label::new(Some(feat));
        lbl.add_css_class("feature-label");

        row.append(&check);
        row.append(&lbl);
        feat_box.append(&row);
    }

    // ── Action buttons ─────────────────────────────────────────────────
    let actions_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    actions_box.set_halign(gtk4::Align::Center);
    actions_box.set_margin_bottom(32);

    let btn_data: &[(&str, &str, &str)] = &[
        ("network-wireless-symbolic",  "Connect WiFi",   "arka-wifi"),
        ("applications-internet-symbolic", "Open Browser", "firefox"),
        ("system-software-install-symbolic", "Install Apps", "arka-capsule"),
        ("security-high-symbolic",     "Privacy Details", "arka-dashboard"),
    ];

    for (icon, label, cmd) in btn_data {
        let btn = gtk4::Button::new();
        btn.add_css_class("action-btn");
        btn.add_css_class("suggested-action");

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        vbox.set_halign(gtk4::Align::Center);

        let img = gtk4::Image::from_icon_name(icon);
        img.set_pixel_size(24);

        let lbl = gtk4::Label::new(Some(label));
        lbl.set_wrap(true);
        lbl.set_justify(gtk4::Justification::Center);
        lbl.set_max_width_chars(12);

        vbox.append(&img);
        vbox.append(&lbl);
        btn.set_child(Some(&vbox));

        let cmd_owned = cmd.to_string();
        btn.connect_clicked(move |_| {
            spawn(&cmd_owned);
        });

        actions_box.append(&btn);
    }

    // ── Get Started button ─────────────────────────────────────────────
    let start_btn = gtk4::Button::with_label("Get Started  →");
    start_btn.add_css_class("get-started");
    start_btn.add_css_class("suggested-action");
    start_btn.set_margin_start(80);
    start_btn.set_margin_end(80);
    start_btn.set_margin_bottom(40);

    let app_clone = app.clone();
    start_btn.connect_clicked(move |_| {
        mark_shown();
        app_clone.quit();
    });

    // Separator line
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_start(48);
    sep.set_margin_end(48);
    sep.set_margin_bottom(24);

    root.append(&brand_box);
    root.append(&feat_box);
    root.append(&sep);
    root.append(&actions_box);
    root.append(&start_btn);

    window.set_content(Some(&root));

    // Esc = dismiss (same as Get Started)
    let ctl = gtk4::EventControllerKey::new();
    let app2 = app.clone();
    ctl.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            mark_shown();
            app2.quit();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(ctl);

    window.present();
}
