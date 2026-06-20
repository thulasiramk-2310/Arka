mod apps;

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Entry, Image, Label,
    ListBox, ListBoxRow, Orientation, ScrolledWindow, SelectionMode,
};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

const APP_ID: &str = "org.arka.launcher";

const STYLE: &str = "
window {
    background: transparent;
}
.launcher-bg {
    background-color: #0d0d14ee;
    border-radius: 12px;
    border: 1px solid #1a2a4a;
    padding: 8px;
    min-width: 480px;
    max-width: 480px;
}
.search-box {
    background-color: #1a1a24;
    border-radius: 8px;
    border: 1px solid #2a3a5a;
    padding: 10px 14px;
    font-size: 16px;
    color: #d0dff0;
    margin-bottom: 8px;
    caret-color: #4fc3f7;
}
.search-box:focus {
    border-color: #4fc3f7;
    outline: none;
}
.app-list {
    background: transparent;
}
.app-row {
    background-color: transparent;
    border-radius: 6px;
    padding: 2px 0;
}
.app-row:hover, .app-row:selected {
    background-color: #1a2a44;
}
.app-name { color: #d0dff0; font-size: 14px; }
.app-exec { color: #556688; font-size: 11px; }
.sandbox-badge {
    color: #4ade80;
    font-size: 10px;
    font-weight: 700;
    background-color: rgba(74,222,128,0.12);
    border-radius: 4px;
    padding: 1px 5px;
    margin-left: 6px;
}
.brand-hint { color: #2a3a5a; font-size: 11px; margin-top: 6px; }
scrolledwindow { background: transparent; }
scrolledwindow undershoot { background: transparent; }
";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let all_apps = apps::load();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("arka-launcher")
        .decorated(false)
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Root: full-screen transparent click-to-dismiss layer
    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_halign(gtk4::Align::Center);
    outer.set_valign(gtk4::Align::Start);
    outer.set_margin_top(80);

    let card = GtkBox::new(Orientation::Vertical, 0);
    card.add_css_class("launcher-bg");

    // Search entry
    let entry = Entry::new();
    entry.add_css_class("search-box");
    entry.set_placeholder_text(Some("▲  Search apps…"));
    entry.set_hexpand(true);

    // App list
    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("app-list");

    let scroll = ScrolledWindow::builder()
        .min_content_height(0)
        .max_content_height(320)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    scroll.set_child(Some(&list));

    let hint = Label::new(Some("Super+Space to close · Enter to launch · Esc to dismiss"));
    hint.add_css_class("brand-hint");
    hint.set_halign(gtk4::Align::Center);

    card.append(&entry);
    card.append(&scroll);
    card.append(&hint);
    outer.append(&card);
    window.set_child(Some(&outer));

    // Populate with all apps initially
    populate(&list, &all_apps, "");

    // Filter on search
    let all_apps_ref = all_apps.clone();
    let list_ref = list.clone();
    entry.connect_changed(move |e| {
        populate(&list_ref, &all_apps_ref, &e.text());
    });

    // Launch on Enter in search
    let all_apps_ref2 = all_apps.clone();
    let list_ref2 = list.clone();
    let window_ref = window.clone();
    entry.connect_activate(move |e| {
        let query = e.text();
        let filtered: Vec<_> = all_apps_ref2.iter()
            .filter(|a| query.is_empty() || a.matches(&query))
            .collect();
        if let Some(first) = filtered.first() {
            launch(&first.exec);
            window_ref.close();
        } else if let Some(row) = list_ref2.selected_row() {
            if let Some(exec) = row.widget_name().as_str().strip_prefix("exec:") {
                launch(exec);
                window_ref.close();
            }
        }
    });

    // Launch on row activate (click or Enter on list)
    let window_ref2 = window.clone();
    list.connect_row_activated(move |_, row| {
        if let Some(exec) = row.widget_name().as_str().strip_prefix("exec:") {
            launch(exec);
            window_ref2.close();
        }
    });

    // Escape closes
    let controller = gtk4::EventControllerKey::new();
    let window_ref3 = window.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            window_ref3.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);

    window.present();
    entry.grab_focus();
}

fn populate(list: &ListBox, apps: &[apps::AppEntry], query: &str) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let filtered: Vec<_> = if query.is_empty() {
        apps.iter().collect()
    } else {
        apps.iter().filter(|a| a.matches(query)).collect()
    };

    for app in filtered.iter().take(12) {
        let row = ListBoxRow::new();
        row.set_widget_name(&format!("exec:{}", app.exec));
        row.add_css_class("app-row");

        let hbox = GtkBox::new(Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(7);
        hbox.set_margin_bottom(7);

        // Icon
        let icon = if app.icon.is_empty() {
            Image::from_icon_name("application-x-executable-symbolic")
        } else {
            Image::from_icon_name(&app.icon)
        };
        icon.set_icon_size(gtk4::IconSize::Large);
        icon.set_pixel_size(28);
        hbox.append(&icon);

        // Name + exec
        let vbox = GtkBox::new(Orientation::Vertical, 1);
        vbox.set_hexpand(true);

        let name_box = GtkBox::new(Orientation::Horizontal, 0);
        let name_lbl = Label::new(Some(&app.name));
        name_lbl.set_halign(gtk4::Align::Start);
        name_lbl.add_css_class("app-name");
        name_box.append(&name_lbl);

        if app.sandboxed {
            let badge = Label::new(Some("SANDBOX"));
            badge.add_css_class("sandbox-badge");
            badge.set_valign(gtk4::Align::Center);
            name_box.append(&badge);
        }

        let exec_lbl = Label::new(Some(&app.exec));
        exec_lbl.set_halign(gtk4::Align::Start);
        exec_lbl.add_css_class("app-exec");

        vbox.append(&name_box);
        vbox.append(&exec_lbl);
        hbox.append(&vbox);

        row.set_child(Some(&hbox));
        list.append(&row);
    }
}

fn launch(exec: &str) {
    let parts: Vec<&str> = exec.split_whitespace().collect();
    if parts.is_empty() { return; }
    let _ = std::process::Command::new(parts[0])
        .args(&parts[1..])
        .spawn();
}
