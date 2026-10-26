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
    arka_shell_common::app::activate_once(&app, build_ui);
    std::process::exit(app.run().value());
}

const CSS: &str = "
.wl-title { font-family: 'Noto Serif', serif; font-size: 36px; font-weight: 500; }
.wl-sub { font-size: 15px; color: alpha(@window_fg_color, 0.75); }
.wl-row { padding: 14px 4px; border-top: 1px solid alpha(@window_fg_color, 0.12); }
.wl-rows { border-bottom: 1px solid alpha(@window_fg_color, 0.12); }
.wl-row-title { font-size: 15px; font-weight: 600; }
.wl-row-sub { font-size: 13px; color: alpha(@window_fg_color, 0.75); }
.wl-note { font-size: 13px; color: alpha(@window_fg_color, 0.75); }
button.wl-row-btn { background: none; box-shadow: none; padding: 0; border-radius: 0; }
button.wl-row-btn:hover .wl-row { background-color: alpha(@window_fg_color, 0.04); }
button.wl-done { border-radius: 999px; padding: 10px 26px; font-weight: 600;
                 background-color: @window_fg_color; color: @window_bg_color; }
";

fn build_ui(app: &adw::Application) {
    // Follow the desktop look (Light/Dark) instead of forcing dark.
    arka_shell_common::theme::follow_look();
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Welcome to ArkaOS")
        .default_width(620)
        .default_height(660)
        .build();

    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    col.set_margin_top(40);
    col.set_margin_bottom(28);
    col.set_margin_start(40);
    col.set_margin_end(40);

    let label = |t: &str, c: &str| {
        let l = gtk4::Label::new(Some(t));
        l.add_css_class(c);
        l.set_xalign(0.0);
        l.set_wrap(true);
        l
    };
    col.append(&label("Welcome to ArkaOS", "wl-title"));
    let sub = label("A few places to start. You can come back to all of these from the app launcher.", "wl-sub");
    sub.set_margin_top(10);
    col.append(&sub);

    // Shortcuts: one row each, title + what it opens.
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rows.add_css_class("wl-rows");
    rows.set_margin_top(28);
    let items: &[(&str, &str, &str, &str)] = &[
        ("network-wireless-symbolic", "Connect to Wi-Fi", "Choose a network and sign in.", "arka-wifi"),
        ("applications-internet-symbolic", "Open Firefox", "Runs in a sandbox that sees only your Downloads folder.", "firefox"),
        ("system-software-install-symbolic", "Install apps", "Browse and install apps with Capsule.", "arka-capsule"),
        ("security-high-symbolic", "Review privacy settings", "See which privacy settings ArkaOS has in place right now.", "arka-dashboard"),
    ];
    for (icon, title, what, cmd) in items {
        let r = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        r.add_css_class("wl-row");
        let img = gtk4::Image::from_icon_name(icon);
        img.set_pixel_size(20);
        img.set_valign(gtk4::Align::Center);
        let t = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        t.set_hexpand(true);
        t.append(&label(title, "wl-row-title"));
        t.append(&label(what, "wl-row-sub"));
        let go = gtk4::Image::from_icon_name("go-next-symbolic");
        go.set_valign(gtk4::Align::Center);
        r.append(&img);
        r.append(&t);
        r.append(&go);
        let b = gtk4::Button::new();
        b.add_css_class("wl-row-btn");
        b.set_child(Some(&r));
        let cmd = cmd.to_string();
        b.connect_clicked(move |_| spawn(&cmd));
        rows.append(&b);
    }
    col.append(&rows);

    let note = label(
        "ArkaOS sets a few privacy settings for you. The Privacy dashboard lists exactly which ones, \
         and whether each is in place.",
        "wl-note",
    );
    note.set_margin_top(18);
    col.append(&note);

    let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    col.append(&spacer);

    let done = gtk4::Button::with_label("Done");
    done.add_css_class("wl-done");
    done.set_halign(gtk4::Align::End);
    done.set_margin_top(24);
    let app_clone = app.clone();
    done.connect_clicked(move |_| {
        mark_shown();
        app_clone.quit();
    });
    col.append(&done);

    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&col)
        .build();
    let tb = adw::ToolbarView::new();
    let hb = adw::HeaderBar::new();
    hb.set_show_title(false);
    tb.add_top_bar(&hb);
    tb.set_content(Some(&scroll));
    window.set_content(Some(&tb));

    // Esc = dismiss (same as Done)
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
    done.grab_focus();
    arka_shell_common::theme::preview_shot(&window);
}
