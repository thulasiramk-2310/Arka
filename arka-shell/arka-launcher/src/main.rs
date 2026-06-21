mod apps;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Entry, Image, Label,
    ListBox, ListBoxRow, Orientation, ScrolledWindow, SelectionMode,
};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

const APP_ID: &str = "org.arka.launcher";

const STYLE: &str = "
window { background: transparent; }

.brand-title {
    color: #4fc3f7;
    font-size: 30px;
    font-weight: 900;
    letter-spacing: 4px;
    text-shadow: 0 0 40px rgba(79,195,247,0.5);
}
.brand-sub {
    color: #2e4d6a;
    font-size: 13px;
    letter-spacing: 1px;
    margin-bottom: 18px;
}
.launcher-bg {
    background-color: rgba(8, 12, 26, 0.95);
    border-radius: 14px;
    border: 1px solid rgba(40, 80, 140, 0.5);
    padding: 12px;
    min-width: 580px;
    max-width: 580px;
}
.search-box {
    background-color: rgba(16, 26, 50, 0.95);
    border-radius: 8px;
    border: 1px solid rgba(50, 90, 160, 0.7);
    padding: 10px 14px;
    font-size: 15px;
    color: #d0dff0;
    margin-bottom: 6px;
    caret-color: #4fc3f7;
}
.search-box:focus { border-color: #4fc3f7; }
.app-list { background: transparent; }
.app-list row { background: transparent; border-radius: 8px; }
.app-list row:hover    { background-color: rgba(20, 55, 100, 0.7); }
.app-list row:selected { background-color: rgba(20, 55, 100, 0.5); }
.app-name  { color: #d8e8f8; font-size: 14px; font-weight: 600; }
.app-exec  { color: #2e4d6a; font-size: 11px; }
.chevron   { color: #2a4a6a; font-size: 18px; margin-left: 6px; }
.sandbox-badge {
    color: #4ade80;
    font-size: 10px;
    font-weight: 700;
    background-color: rgba(74,222,128,0.12);
    border-radius: 4px;
    padding: 1px 5px;
    margin-left: 6px;
}
.kbd-key {
    border: 1px solid #1e3a5a;
    border-radius: 3px;
    padding: 0 5px;
    font-size: 10px;
    color: #2e5070;
    background-color: rgba(30,60,100,0.2);
}
.hint-bar { color: #1e3050; font-size: 11px; }
.dot-active { color: #4fc3f7; font-size: 9px; }
.dot-dim    { color: #1a2a3a; font-size: 9px; }
scrolledwindow { background: transparent; }
scrolledwindow undershoot { background: transparent; }
separator { background-color: rgba(30, 60, 110, 0.25); margin: 0 10px; }
";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let all_apps = apps::load();
    let filtered: Rc<RefCell<Vec<apps::AppEntry>>> = Rc::new(RefCell::new(all_apps.clone()));

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

    // Outer column: brand + card + dots, centered horizontally, ~1/3 from top
    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_halign(gtk4::Align::Center);
    outer.set_valign(gtk4::Align::Start);
    outer.set_margin_top(110);

    // Brand above card
    let brand_title = Label::new(Some("▲  ARKA"));
    brand_title.add_css_class("brand-title");
    brand_title.set_halign(gtk4::Align::Center);

    let brand_sub = Label::new(Some("Your Computer Is Yours"));
    brand_sub.add_css_class("brand-sub");
    brand_sub.set_halign(gtk4::Align::Center);

    // Card
    let card = GtkBox::new(Orientation::Vertical, 0);
    card.add_css_class("launcher-bg");

    // Search entry
    let entry = Entry::new();
    entry.add_css_class("search-box");
    entry.set_placeholder_text(Some("▲  Search apps..."));
    entry.set_hexpand(true);

    // App list with separators
    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.set_show_separators(true);
    list.add_css_class("app-list");

    let scroll = ScrolledWindow::builder()
        .min_content_height(0)
        .max_content_height(320)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    scroll.set_child(Some(&list));

    // Hint bar
    let hint_box = GtkBox::new(Orientation::Horizontal, 4);
    hint_box.set_halign(gtk4::Align::Center);
    hint_box.set_margin_top(8);

    let super_key = Label::new(Some("Super"));
    super_key.add_css_class("kbd-key");
    let hint_rest = Label::new(Some(" + Number to open  •  ↑↓ to navigate  •  Enter to launch  •  Esc to close"));
    hint_rest.add_css_class("hint-bar");
    hint_box.append(&super_key);
    hint_box.append(&hint_rest);

    card.append(&entry);
    card.append(&scroll);
    card.append(&hint_box);

    // Pagination dots
    let dots = GtkBox::new(Orientation::Horizontal, 8);
    dots.set_halign(gtk4::Align::Center);
    dots.set_margin_top(14);
    for i in 0..3 {
        let dot = Label::new(Some("●"));
        dot.add_css_class(if i == 0 { "dot-active" } else { "dot-dim" });
        dots.append(&dot);
    }

    outer.append(&brand_title);
    outer.append(&brand_sub);
    outer.append(&card);
    outer.append(&dots);
    window.set_child(Some(&outer));

    // Initial populate
    populate(&list, &all_apps);

    // Filter on search
    let all_apps_ref = all_apps.clone();
    let list_ref = list.clone();
    let filtered_ref = filtered.clone();
    entry.connect_changed(move |e| {
        let query = e.text();
        let new_filtered: Vec<_> = if query.is_empty() {
            all_apps_ref.clone()
        } else {
            all_apps_ref.iter().filter(|a| a.matches(&query)).cloned().collect()
        };
        populate(&list_ref, &new_filtered);
        *filtered_ref.borrow_mut() = new_filtered;
    });

    // Enter launches first result
    let filtered_ref2 = filtered.clone();
    let window_ref = window.clone();
    entry.connect_activate(move |_| {
        let fa = filtered_ref2.borrow();
        if let Some(first) = fa.first() {
            launch(&first.exec);
            window_ref.close();
        }
    });

    // Row click
    let window_ref2 = window.clone();
    list.connect_row_activated(move |_, row| {
        if let Some(exec) = row.widget_name().as_str().strip_prefix("exec:") {
            launch(exec);
            window_ref2.close();
        }
    });

    // Key handler: Escape + 1–9 shortcuts
    let controller = gtk4::EventControllerKey::new();
    let window_ref3 = window.clone();
    let filtered_ref3 = filtered.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            window_ref3.close();
            return glib::Propagation::Stop;
        }
        if let Some(idx) = key_to_index(key) {
            let fa = filtered_ref3.borrow();
            if let Some(app) = fa.get(idx) {
                launch(&app.exec);
                window_ref3.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(controller);

    window.present();
    entry.grab_focus();
}

fn populate(list: &ListBox, apps: &[apps::AppEntry]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for app in apps.iter().take(9) {
        let row = ListBoxRow::new();
        row.set_widget_name(&format!("exec:{}", app.exec));

        let hbox = GtkBox::new(Orientation::Horizontal, 12);
        hbox.set_margin_start(14);
        hbox.set_margin_end(14);
        hbox.set_margin_top(10);
        hbox.set_margin_bottom(10);

        // Icon
        let icon = if app.icon.is_empty() {
            Image::from_icon_name("application-x-executable-symbolic")
        } else {
            Image::from_icon_name(&app.icon)
        };
        icon.set_pixel_size(28);
        hbox.append(&icon);

        // Name + exec
        let vbox = GtkBox::new(Orientation::Vertical, 2);
        vbox.set_hexpand(true);

        let name_row = GtkBox::new(Orientation::Horizontal, 0);
        let name_lbl = Label::new(Some(&app.name));
        name_lbl.set_halign(gtk4::Align::Start);
        name_lbl.add_css_class("app-name");
        name_row.append(&name_lbl);
        if app.sandboxed {
            let badge = Label::new(Some("SANDBOX"));
            badge.add_css_class("sandbox-badge");
            badge.set_valign(gtk4::Align::Center);
            name_row.append(&badge);
        }

        let exec_lbl = Label::new(Some(&app.exec));
        exec_lbl.set_halign(gtk4::Align::Start);
        exec_lbl.add_css_class("app-exec");

        vbox.append(&name_row);
        vbox.append(&exec_lbl);
        hbox.append(&vbox);

        // Chevron
        let chevron = Label::new(Some("›"));
        chevron.add_css_class("chevron");
        chevron.set_valign(gtk4::Align::Center);
        hbox.append(&chevron);

        row.set_child(Some(&hbox));
        list.append(&row);
    }
}

fn key_to_index(key: gtk4::gdk::Key) -> Option<usize> {
    use gtk4::gdk::Key;
    match key {
        Key::_1 => Some(0), Key::_2 => Some(1), Key::_3 => Some(2),
        Key::_4 => Some(3), Key::_5 => Some(4), Key::_6 => Some(5),
        Key::_7 => Some(6), Key::_8 => Some(7), Key::_9 => Some(8),
        _ => None,
    }
}

fn launch(exec: &str) {
    let parts: Vec<&str> = exec.split_whitespace().collect();
    if parts.is_empty() { return; }
    let _ = std::process::Command::new(parts[0]).args(&parts[1..]).spawn();
}
