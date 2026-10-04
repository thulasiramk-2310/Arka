use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

const APP_ID: &str = "org.arka.hotkeys";

/// KDE Plasma 6 default shortcuts only — ArkaOS adds none of its own yet, so
/// this sheet must never list a key that doesn't work. (Meta = the Windows /
/// Super key.) Change these in System Settings > Keyboard > Shortcuts.
const HOTKEYS: &[(&str, &[(&str, &str)])] = &[
    ("Find things", &[
        ("Meta",           "Open the app launcher"),
        ("Alt + Space",    "Search apps, files and settings (KRunner)"),
    ]),
    ("Windows", &[
        ("Alt + Tab",      "Switch between windows"),
        ("Meta + W",       "Overview of all windows and desktops"),
        ("Meta + D",       "Peek at the desktop"),
        ("Meta + Left / Right", "Snap a window to the left or right half"),
        ("Alt + F4",       "Close the window"),
    ]),
    ("Apps", &[
        ("Ctrl + Alt + T", "Open a terminal"),
        ("Meta + E",       "Open the file manager"),
        ("Meta + V",       "Clipboard history"),
    ]),
    ("Screenshots", &[
        ("Print",          "Screenshot tool"),
        ("Meta + Shift + S", "Screenshot a selected area"),
    ]),
    ("Desktops", &[
        ("Ctrl + F1 – F4", "Switch to virtual desktop 1 – 4"),
    ]),
    ("Session", &[
        ("Meta + L",       "Lock the screen"),
        ("Ctrl + Alt + Del", "Log out, restart or shut down"),
    ]),
];

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
        .title("Keyboard Shortcuts")
        .default_width(560)
        .default_height(640)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(".kbd { font-family: 'JetBrains Mono', monospace; font-size: 12px; color: @text_lo; \
        padding: 2px 8px; border: 1px solid @border_ui; border-radius: 6px; }");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_show_title(false);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_start(28);
    content.set_margin_end(28);
    content.set_margin_top(20);
    content.set_margin_bottom(28);
    let title = gtk4::Label::new(Some("Keyboard shortcuts"));
    title.add_css_class("arka-title");
    title.set_xalign(0.0);
    let sub = gtk4::Label::new(Some("The standard KDE Plasma shortcuts. Meta is the Windows key. Change them in System Settings > Keyboard > Shortcuts."));
    sub.add_css_class("arka-sub");
    sub.set_xalign(0.0);
    sub.set_wrap(true);
    content.append(&title);
    content.append(&sub);

    for (section, binds) in HOTKEYS {
        let group = adw::PreferencesGroup::new();
        group.set_title(section);
        for (key, desc) in *binds {
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
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
    arka_shell_common::theme::preview_shot(&window);
}
