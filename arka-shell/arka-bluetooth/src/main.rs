use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::process::Command;

const APP_ID: &str = "org.arka.bluetooth";

// ── bluetoothctl helpers ────────────────────────────────────────────────────

#[derive(Clone)]
struct BtDevice {
    addr: String,
    name: String,
    connected: bool,
    paired: bool,
}

fn bt_powered() -> bool {
    let out = Command::new("bluetoothctl")
        .args(["show"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    out.lines().any(|l| l.contains("Powered: yes"))
}

fn bt_set_power(on: bool) {
    let _ = Command::new("bluetoothctl")
        .args(["power", if on { "on" } else { "off" }])
        .output();
}

fn bt_list_devices() -> Vec<BtDevice> {
    let paired_out = Command::new("bluetoothctl")
        .args(["devices", "Paired"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let conn_out = Command::new("bluetoothctl")
        .args(["devices", "Connected"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let connected_addrs: Vec<&str> = conn_out.lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .collect();

    paired_out.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() < 3 { return None; }
            let addr = parts[1].to_string();
            let name = parts[2].to_string();
            let connected = connected_addrs.contains(&addr.as_str());
            Some(BtDevice { addr, name, connected, paired: true })
        })
        .collect()
}

fn bt_scan_nearby() -> Vec<BtDevice> {
    // Quick 5-second scan, return all devices found
    let _ = Command::new("bluetoothctl").args(["scan", "on"]).spawn();
    std::thread::sleep(std::time::Duration::from_secs(5));
    let _ = Command::new("bluetoothctl").args(["scan", "off"]).spawn();

    let out = Command::new("bluetoothctl")
        .args(["devices"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let paired = bt_list_devices();
    let paired_addrs: Vec<&str> = paired.iter().map(|d| d.addr.as_str()).collect();

    out.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() < 3 { return None; }
            let addr = parts[1].to_string();
            if addr.starts_with("Device") { return None; }
            let name = parts[2].to_string();
            let already_paired = paired_addrs.contains(&addr.as_str());
            Some(BtDevice { addr, name, connected: false, paired: already_paired })
        })
        .collect()
}

fn bt_connect(addr: &str) {
    let _ = Command::new("bluetoothctl").args(["connect", addr]).spawn();
}

fn bt_disconnect(addr: &str) {
    let _ = Command::new("bluetoothctl").args(["disconnect", addr]).spawn();
}

fn bt_remove(addr: &str) {
    let _ = Command::new("bluetoothctl").args(["remove", addr]).spawn();
}

// ── UI ───────────────────────────────────────────────────────────────────────

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .status-connected { color: #4fc3f7; font-size: 11px; }
    .status-paired    { color: #4a6a88; font-size: 11px; }
    .status-new       { color: #4ade80; font-size: 11px; }
    .scan-spinner     { color: #4fc3f7; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Bluetooth")
        .default_width(420)
        .default_height(500)
        .resizable(false)
        .build();

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("Bluetooth", "ArkaOS")));

    // Power toggle in header
    let power_switch = gtk4::Switch::new();
    power_switch.set_valign(gtk4::Align::Center);
    power_switch.set_active(bt_powered());
    hb.pack_end(&power_switch);

    // Scan button
    let scan_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
    scan_btn.set_tooltip_text(Some("Scan for devices"));
    hb.pack_start(&scan_btn);

    // Main content
    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    // Paired devices group
    let paired_group = adw::PreferencesGroup::new();
    paired_group.set_title("My Devices");

    // Nearby group (shown after scan)
    let nearby_group = adw::PreferencesGroup::new();
    nearby_group.set_title("Nearby Devices");
    nearby_group.set_visible(false);

    content.append(&paired_group);
    content.append(&nearby_group);
    scroll.set_child(Some(&content));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&scroll));
    window.set_content(Some(&toolbar));

    // Populate paired devices
    populate_paired(&paired_group);

    // Power switch
    let content_ref = content.clone();
    power_switch.connect_active_notify(move |sw| {
        bt_set_power(sw.is_active());
        content_ref.set_sensitive(sw.is_active());
    });
    content.set_sensitive(bt_powered());

    // Scan button
    let paired_ref = paired_group.clone();
    let nearby_ref = nearby_group.clone();
    let scan_ref = scan_btn.clone();
    scan_btn.connect_clicked(move |_| {
        scan_ref.set_sensitive(false);
        let icon = gtk4::Spinner::new();
        icon.start();
        scan_ref.set_child(Some(&icon));

        // Clear nearby
        while let Some(child) = nearby_ref.first_child() {
            nearby_ref.remove(&child);
        }

        let nearby_ref2 = nearby_ref.clone();
        let paired_ref2 = paired_ref.clone();
        let scan_ref2 = scan_ref.clone();
        glib::spawn_future_local(async move {
            let devices = gtk4::gio::spawn_blocking(bt_scan_nearby).await.unwrap_or_default();

            // Refresh paired list too
            while let Some(child) = paired_ref2.first_child() {
                paired_ref2.remove(&child);
            }
            populate_paired(&paired_ref2);

            // Show nearby (unpaired) devices
            let unpaired: Vec<_> = devices.iter().filter(|d| !d.paired).cloned().collect();
            if unpaired.is_empty() {
                let row = adw::ActionRow::new();
                row.set_title("No new devices found");
                row.set_subtitle("Make sure the device is in pairing mode");
                nearby_ref2.add(&row);
            } else {
                for dev in unpaired {
                    nearby_ref2.add(&make_nearby_row(&dev, &paired_ref2));
                }
            }
            nearby_ref2.set_visible(true);

            scan_ref2.set_child(gtk4::Widget::NONE);
            scan_ref2.set_icon_name("view-refresh-symbolic");
            scan_ref2.set_sensitive(true);
        });
    });

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

fn populate_paired(group: &adw::PreferencesGroup) {
    let devices = bt_list_devices();
    if devices.is_empty() {
        let row = adw::ActionRow::new();
        row.set_title("No paired devices");
        row.set_subtitle("Click the refresh button to scan");
        group.add(&row);
        return;
    }
    for dev in devices {
        group.add(&make_paired_row(&dev, group));
    }
}

fn make_paired_row(dev: &BtDevice, group: &adw::PreferencesGroup) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&dev.name);
    row.set_subtitle(&dev.addr);

    let icon_name = bt_device_icon(&dev.name);
    let icon = gtk4::Image::from_icon_name(icon_name);
    row.add_prefix(&icon);

    let status = gtk4::Label::new(Some(if dev.connected { "Connected" } else { "Paired" }));
    status.add_css_class(if dev.connected { "status-connected" } else { "status-paired" });
    row.add_suffix(&status);

    let btn_label = if dev.connected { "Disconnect" } else { "Connect" };
    let btn = gtk4::Button::with_label(btn_label);
    btn.set_valign(gtk4::Align::Center);
    if dev.connected {
        btn.add_css_class("destructive-action");
    } else {
        btn.add_css_class("suggested-action");
    }
    btn.add_css_class("pill");
    row.add_suffix(&btn);

    let addr = dev.addr.clone();
    let connected = dev.connected;
    let group_ref = group.clone();
    btn.connect_clicked(move |_| {
        if connected {
            bt_disconnect(&addr);
        } else {
            bt_connect(&addr);
        }
        // Refresh after short delay
        let group2 = group_ref.clone();
        glib::timeout_add_seconds_local(2, move || {
            while let Some(child) = group2.first_child() {
                group2.remove(&child);
            }
            populate_paired(&group2);
            glib::ControlFlow::Break
        });
    });

    // Long-press / right-click → forget
    let menu_btn = gtk4::MenuButton::new();
    menu_btn.set_icon_name("view-more-symbolic");
    menu_btn.set_valign(gtk4::Align::Center);
    let menu = gtk4::PopoverMenu::from_model(None::<&gtk4::gio::MenuModel>);
    let forget_btn = gtk4::Button::with_label("Forget Device");
    forget_btn.add_css_class("destructive-action");
    forget_btn.set_margin_start(8);
    forget_btn.set_margin_end(8);
    forget_btn.set_margin_top(4);
    forget_btn.set_margin_bottom(4);
    let addr2 = dev.addr.clone();
    let group3 = group.clone();
    forget_btn.connect_clicked(move |_| {
        bt_remove(&addr2);
        let group4 = group3.clone();
        glib::timeout_add_seconds_local(1, move || {
            while let Some(child) = group4.first_child() {
                group4.remove(&child);
            }
            populate_paired(&group4);
            glib::ControlFlow::Break
        });
    });
    menu.set_child(Some(&forget_btn));
    menu_btn.set_popover(Some(&menu));
    row.add_suffix(&menu_btn);

    row
}

fn make_nearby_row(dev: &BtDevice, paired_group: &adw::PreferencesGroup) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&dev.name);
    row.set_subtitle(&dev.addr);

    let icon_name = bt_device_icon(&dev.name);
    let icon = gtk4::Image::from_icon_name(icon_name);
    row.add_prefix(&icon);

    let status = gtk4::Label::new(Some("Available"));
    status.add_css_class("status-new");
    row.add_suffix(&status);

    let pair_btn = gtk4::Button::with_label("Pair");
    pair_btn.set_valign(gtk4::Align::Center);
    pair_btn.add_css_class("suggested-action");
    pair_btn.add_css_class("pill");
    row.add_suffix(&pair_btn);

    let addr = dev.addr.clone();
    let paired_ref = paired_group.clone();
    pair_btn.connect_clicked(move |btn| {
        btn.set_label("Pairing…");
        btn.set_sensitive(false);
        bt_connect(&addr);
        let paired2 = paired_ref.clone();
        glib::timeout_add_seconds_local(3, move || {
            while let Some(child) = paired2.first_child() {
                paired2.remove(&child);
            }
            populate_paired(&paired2);
            glib::ControlFlow::Break
        });
    });

    row
}

fn bt_device_icon(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("headphone") || lower.contains("airpod") || lower.contains("headset")
        || lower.contains("buds") || lower.contains("ear") {
        "audio-headphones-symbolic"
    } else if lower.contains("speaker") || lower.contains("soundbar") {
        "audio-speakers-symbolic"
    } else if lower.contains("keyboard") {
        "input-keyboard-symbolic"
    } else if lower.contains("mouse") || lower.contains("trackpad") {
        "input-mouse-symbolic"
    } else if lower.contains("phone") {
        "phone-symbolic"
    } else if lower.contains("controller") || lower.contains("gamepad") || lower.contains("xbox")
        || lower.contains("playstation") || lower.contains("ds4") {
        "input-gaming-symbolic"
    } else {
        "bluetooth-symbolic"
    }
}
