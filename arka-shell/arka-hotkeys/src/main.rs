use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

const APP_ID: &str = "org.arka.hotkeys";

const HOTKEYS: &[(&str, &[(&str, &str)])] = &[
    ("Apps", &[
        ("Super + Space",   "Open app launcher"),
        ("Super + S",       "Settings"),
        ("Super + D",       "Privacy dashboard"),
        ("Super + W",       "WiFi networks"),
        ("Super + U",       "System updates"),
        ("Super + P",       "Capsule — install apps"),
        ("Super + M",       "App permissions"),
        ("Super + N",       "Sound & volume"),
        ("Super + T",       "Bluetooth"),
        ("Super + /",       "This cheat sheet"),
    ]),
    ("Windows", &[
        ("Super + Enter",   "Open terminal"),
        ("Super + B",       "Open browser"),
        ("Super + E",       "File manager"),
        ("Super + Shift+Q", "Close window"),
        ("Alt + F4",        "Close window"),
        ("Super + F",       "Fullscreen"),
        ("Super + V",       "Toggle floating"),
    ]),
    ("Power", &[
        ("Super + Space → Sleep",     "Suspend the system"),
        ("Super + Space → Lock",      "Lock the screen"),
        ("Super + Space → Restart",   "Reboot"),
        ("Super + Space → Shut Down", "Power off"),
    ]),
    ("Media Keys", &[
        ("Volume Up/Down",  "Raise / lower volume 5%"),
        ("Mute",            "Toggle mute"),
        ("Brightness Up",   "Increase screen brightness"),
        ("Brightness Down", "Decrease screen brightness"),
    ]),
    ("Screenshots", &[
        ("Print",           "Screenshot entire screen"),
        ("Super + Shift+S", "Screenshot selected area"),
    ]),
    ("Volume", &[
        ("F3 / Volume Up",   "Raise volume 5%"),
        ("F2 / Volume Down", "Lower volume 5%"),
        ("F4 / Mute",        "Toggle mute"),
    ]),
    ("Workspaces", &[
        ("Super + 1–3",     "Switch workspace"),
        ("Super+Shift+1–3", "Move window to workspace"),
    ]),
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
        .title("Keyboard Shortcuts")
        .default_width(480)
        .default_height(560)
        .resizable(false)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(".kbd { font-family: monospace; font-size: 12px; color: #4fc3f7; }");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("Keyboard Shortcuts", "ArkaOS")));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    for (section, binds) in HOTKEYS {
        let group = adw::PreferencesGroup::new();
        group.set_title(section);
        for (key, desc) in *binds {
            let row = adw::ActionRow::new();
            row.set_title(desc);
            let key_lbl = gtk4::Label::new(Some(key));
            key_lbl.add_css_class("kbd");
            key_lbl.set_valign(gtk4::Align::Center);
            row.add_suffix(&key_lbl);
            group.add(&row);
        }
        content.append(&group);
    }

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&scroll));
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
