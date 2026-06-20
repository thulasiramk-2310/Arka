use arka_shell_common::DnsStatus;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

const APP_ID: &str = "org.arka.bar";

const STYLE: &str = "
.arka-bar { background-color: #0f0f14; padding: 2px 12px; }
.brand          { color: #4fc3f7; font-weight: bold; letter-spacing: 2px; }
.privacy-score  { color: #8bd17c; margin: 0 14px; }
.privacy-warn   { color: #e5a445; margin: 0 14px; }
.wifi-on        { color: #8bd17c; margin: 0 10px; }
.wifi-off       { color: #555566; margin: 0 10px; }
.battery-ok     { color: #cfcfcf; margin: 0 10px; }
.battery-low    { color: #e5a445; margin: 0 10px; }
.battery-charge { color: #4fc3f7; margin: 0 10px; }
.clock          { color: #cfcfcf; }
";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("arka-bar")
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.auto_exclusive_zone_enable();
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);

    let root = GtkBox::new(Orientation::Horizontal, 0);
    root.add_css_class("arka-bar");

    let brand = Label::new(Some("▲ ARKA"));
    brand.add_css_class("brand");

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let privacy_label = Label::new(Some("Privacy --"));
    privacy_label.add_css_class("privacy-score");

    let wifi_label = Label::new(None);
    let battery_label = Label::new(None);

    let clock_label = Label::new(Some(""));
    clock_label.add_css_class("clock");

    root.append(&brand);
    root.append(&spacer);
    root.append(&privacy_label);
    root.append(&wifi_label);
    root.append(&battery_label);
    root.append(&clock_label);

    window.set_child(Some(&root));

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display connection"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Clock — every second
    glib::timeout_add_seconds_local(1, {
        let clock_label = clock_label.clone();
        move || {
            clock_label.set_text(&chrono::Local::now().format("%H:%M  %Y-%m-%d").to_string());
            glib::ControlFlow::Continue
        }
    });

    // Privacy — every 5s
    update_privacy(&privacy_label);
    glib::timeout_add_seconds_local(5, {
        let privacy_label = privacy_label.clone();
        move || { update_privacy(&privacy_label); glib::ControlFlow::Continue }
    });

    // WiFi + battery — every 10s
    update_wifi(&wifi_label);
    update_battery(&battery_label);
    glib::timeout_add_seconds_local(10, {
        let wifi_label = wifi_label.clone();
        let battery_label = battery_label.clone();
        move || {
            update_wifi(&wifi_label);
            update_battery(&battery_label);
            glib::ControlFlow::Continue
        }
    });

    window.present();
}

fn update_privacy(label: &Label) {
    match fetch_privacy() {
        Ok((score, dns)) => {
            label.set_text(&format!("Privacy {score}"));
            if dns == DnsStatus::Encrypted {
                label.remove_css_class("privacy-warn");
                label.add_css_class("privacy-score");
            } else {
                label.remove_css_class("privacy-score");
                label.add_css_class("privacy-warn");
            }
        }
        Err(_) => {
            label.set_text("Privacy --");
            label.remove_css_class("privacy-warn");
            label.add_css_class("privacy-score");
        }
    }
}

fn update_wifi(label: &Label) {
    match wifi_state() {
        Some(true) => {
            label.set_text("WiFi");
            label.remove_css_class("wifi-off");
            label.add_css_class("wifi-on");
        }
        Some(false) => {
            label.set_text("No WiFi");
            label.remove_css_class("wifi-on");
            label.add_css_class("wifi-off");
        }
        None => label.set_text(""),
    }
}

fn update_battery(label: &Label) {
    match battery_state() {
        Some((pct, charging)) => {
            label.set_text(&format!("{pct}%"));
            label.remove_css_class("battery-ok");
            label.remove_css_class("battery-low");
            label.remove_css_class("battery-charge");
            if charging {
                label.add_css_class("battery-charge");
            } else if pct < 20 {
                label.add_css_class("battery-low");
            } else {
                label.add_css_class("battery-ok");
            }
        }
        None => label.set_text(""),
    }
}

fn fetch_privacy() -> zbus::Result<(u8, DnsStatus)> {
    let conn = zbus::blocking::Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.arka.arkad",
        "/org/arka/arkad",
        "org.arka.arkad",
    )?;
    let score = proxy.get_property::<u8>("PrivacyScore")?;
    let dns = proxy
        .get_property::<String>("DnsStatus")
        .map(DnsStatus::from)
        .unwrap_or(DnsStatus::Unknown("?".into()));
    Ok((score, dns))
}

fn wifi_state() -> Option<bool> {
    for entry in std::fs::read_dir("/sys/class/net").ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with('w') { continue; }
        if !std::path::Path::new(&format!("/sys/class/net/{name}/wireless")).exists() {
            continue;
        }
        let state = std::fs::read_to_string(format!("/sys/class/net/{name}/operstate"))
            .unwrap_or_default();
        return Some(state.trim() == "up");
    }
    None
}

fn battery_state() -> Option<(u8, bool)> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("BAT") { continue; }
        let base = format!("/sys/class/power_supply/{name}");
        let pct: u8 = std::fs::read_to_string(format!("{base}/capacity"))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let status = std::fs::read_to_string(format!("{base}/status")).unwrap_or_default();
        return Some((pct, status.trim() == "Charging"));
    }
    None
}
