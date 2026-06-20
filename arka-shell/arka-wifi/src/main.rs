mod net;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

const APP_ID: &str = "org.arka.wifi";

const STYLE: &str = "
.signal-on  { font-family: monospace; color: #4fc3f7; font-size: 12px; }
.signal-off { font-family: monospace; color: #1a3050; font-size: 12px; }
.connected  { color: #4ade80; font-size: 11px; font-weight: 700; }
";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("WiFi Networks")
        .default_width(420)
        .default_height(520)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("WiFi", "Select a network")));
    let refresh_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
    hb.pack_end(&refresh_btn);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_start(16); content.set_margin_end(16);
    content.set_margin_top(16);  content.set_margin_bottom(16);
    content.append(&list);
    scroll.set_child(Some(&content));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&scroll));
    window.set_content(Some(&toolbar));

    // Shared network list — set once on scan, read in the row-click handler
    let networks: Rc<RefCell<Vec<net::Network>>> = Rc::new(RefCell::new(Vec::new()));

    // Wire up row-click once; reads from `networks` each time
    let nets_click = networks.clone();
    let win_click  = window.clone();
    list.connect_row_activated(move |_, row| {
        let ssid = row.widget_name().to_string();
        let guard = nets_click.borrow();
        let Some(n) = guard.iter().find(|n| n.ssid == ssid) else { return };
        if n.in_use { return; }
        if n.secured { show_password_dialog(&win_click, &ssid); }
        else         { do_connect(&win_click, &ssid, None); }
    });

    // First scan
    do_scan(&list, &networks);

    // Refresh button
    let nets_ref = networks.clone();
    let list_ref = list.clone();
    refresh_btn.connect_clicked(move |_| {
        do_scan(&list_ref, &nets_ref);
    });

    window.present();
}

fn do_scan(list: &gtk4::ListBox, networks: &Rc<RefCell<Vec<net::Network>>>) {
    let fresh = net::scan();
    *networks.borrow_mut() = fresh.clone();
    rebuild_rows(list, &fresh);
}

fn rebuild_rows(list: &gtk4::ListBox, networks: &[net::Network]) {
    while let Some(child) = list.first_child() { list.remove(&child); }

    if networks.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let lbl = gtk4::Label::new(Some("No WiFi networks found"));
        lbl.set_margin_top(20); lbl.set_margin_bottom(20);
        row.set_child(Some(&lbl));
        row.set_activatable(false);
        list.append(&row);
        return;
    }

    for n in networks {
        let row = gtk4::ListBoxRow::new();
        row.set_widget_name(&n.ssid);
        row.set_activatable(!n.in_use);

        let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        hbox.set_margin_start(14); hbox.set_margin_end(14);
        hbox.set_margin_top(12);  hbox.set_margin_bottom(12);

        let bars = gtk4::Label::new(Some(net::signal_bars(n.signal)));
        bars.add_css_class(if n.signal > 40 { "signal-on" } else { "signal-off" });
        hbox.append(&bars);

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        vbox.set_hexpand(true);
        let name = gtk4::Label::new(Some(&n.ssid));
        name.set_halign(gtk4::Align::Start);
        name.set_css_classes(&["title-4"]);
        let sub = gtk4::Label::new(Some(if n.secured { "Secured  🔒" } else { "Open" }));
        sub.set_halign(gtk4::Align::Start);
        sub.set_css_classes(&["caption"]);
        vbox.append(&name); vbox.append(&sub);
        hbox.append(&vbox);

        if n.in_use {
            let badge = gtk4::Label::new(Some("Connected ✓"));
            badge.add_css_class("connected");
            badge.set_valign(gtk4::Align::Center);
            hbox.append(&badge);
        } else {
            let arrow = gtk4::Image::from_icon_name("go-next-symbolic");
            arrow.set_pixel_size(14);
            arrow.set_valign(gtk4::Align::Center);
            hbox.append(&arrow);
        }

        row.set_child(Some(&hbox));
        list.append(&row);
    }
}

fn show_password_dialog(parent: &adw::ApplicationWindow, ssid: &str) {
    let win = gtk4::Window::builder()
        .title(&format!("Connect to {ssid}"))
        .transient_for(parent)
        .modal(true)
        .default_width(340)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    vbox.set_margin_start(24); vbox.set_margin_end(24);
    vbox.set_margin_top(24);  vbox.set_margin_bottom(24);

    let lbl = gtk4::Label::new(Some(&format!("Password for \"{}\"", ssid)));
    lbl.set_halign(gtk4::Align::Start);
    lbl.set_css_classes(&["title-4"]);

    let entry = gtk4::Entry::new();
    entry.set_visibility(false);
    entry.set_input_purpose(gtk4::InputPurpose::Password);
    entry.set_placeholder_text(Some("Network password"));
    entry.set_activates_default(true);

    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk4::Align::End);
    btn_row.set_margin_top(4);
    let cancel  = gtk4::Button::with_label("Cancel");
    let connect = gtk4::Button::with_label("Connect");
    connect.add_css_class("suggested-action");
    btn_row.append(&cancel); btn_row.append(&connect);

    vbox.append(&lbl); vbox.append(&entry); vbox.append(&btn_row);
    win.set_child(Some(&vbox));

    let w1 = win.clone();
    cancel.connect_clicked(move |_| w1.close());

    let ssid = ssid.to_string();
    let parent_ref = parent.clone();
    let w2 = win.clone();
    connect.connect_clicked(move |_| {
        let pw = entry.text().to_string();
        w2.close();
        do_connect(&parent_ref, &ssid, if pw.is_empty() { None } else { Some(&pw) });
    });

    win.present();
}

fn do_connect(parent: &adw::ApplicationWindow, ssid: &str, pw: Option<&str>) {
    if let Err(e) = net::connect(ssid, pw) {
        let msg = gtk4::Window::builder()
            .title("Connection Failed").transient_for(parent)
            .modal(true).default_width(320).resizable(false).build();
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        vbox.set_margin_start(24); vbox.set_margin_end(24);
        vbox.set_margin_top(24);  vbox.set_margin_bottom(24);
        let txt = gtk4::Label::new(Some(&e));
        txt.set_wrap(true);
        let btn = gtk4::Button::with_label("OK");
        let m = msg.clone();
        btn.connect_clicked(move |_| m.close());
        vbox.append(&txt); vbox.append(&btn);
        msg.set_child(Some(&vbox));
        msg.present();
    } else {
        parent.close();
    }
}
