use arka_shell_common::DnsStatus;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Image, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

const APP_ID: &str = "org.arka.bar";

const STYLE: &str = "
.arka-bar { background-color: #0f0f14; padding: 2px 12px; }
.brand         { color: #4fc3f7; font-weight: bold; letter-spacing: 2px; }
.privacy-score { color: #8bd17c; margin: 0 14px; }
.privacy-warn  { color: #e5a445; margin: 0 14px; }
.bat-ok        { color: #cfcfcf; margin-right: 8px; }
.bat-low       { color: #e5a445; margin-right: 8px; }
.bat-charge    { color: #4fc3f7; margin-right: 8px; }
.clock         { color: #c8d8f0; margin-left: 10px; }
.date-lbl      { color: #3a5a78; margin-left: 6px; }
image          { margin: 0 4px; }
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
    // ▲ ARKA is the Start Button — click opens launcher
    let brand_click = gtk4::GestureClick::new();
    brand_click.connect_released(|_, _, _, _| {
        let _ = std::process::Command::new("arka-launcher").spawn();
    });
    brand.add_controller(brand_click);

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let privacy_label = Label::new(Some("Privacy --"));
    privacy_label.add_css_class("privacy-score");
    // Click privacy score → open dashboard
    let priv_click = gtk4::GestureClick::new();
    priv_click.connect_released(|_, _, _, _| {
        let _ = std::process::Command::new("arka-dashboard").spawn();
    });
    privacy_label.add_controller(priv_click);

    let wifi_img = Image::from_icon_name("network-wireless-signal-none-symbolic");
    wifi_img.set_pixel_size(16);
    // Click WiFi icon → open network picker
    let wifi_click = gtk4::GestureClick::new();
    wifi_click.connect_released(|_, _, _, _| {
        let _ = std::process::Command::new("arka-wifi").spawn();
    });
    wifi_img.add_controller(wifi_click);

    let vol_img = Image::from_icon_name("audio-volume-high-symbolic");
    vol_img.set_pixel_size(16);
    // Click volume icon → open audio settings
    let vol_gesture = gtk4::GestureClick::new();
    vol_gesture.connect_released(|_, _, _, _| {
        let _ = std::process::Command::new("arka-sound").spawn();
    });
    vol_img.add_controller(vol_gesture);

    let bat_img = Image::from_icon_name("battery-good-symbolic");
    bat_img.set_pixel_size(16);
    let bat_label = Label::new(Some(""));
    bat_label.add_css_class("bat-ok");

    let clock_label = Label::new(Some(""));
    clock_label.add_css_class("clock");

    let date_label = Label::new(Some(""));
    date_label.add_css_class("date-lbl");

    root.append(&brand);
    root.append(&spacer);
    root.append(&privacy_label);
    root.append(&wifi_img);
    root.append(&vol_img);
    root.append(&bat_img);
    root.append(&bat_label);
    root.append(&clock_label);
    root.append(&date_label);

    window.set_child(Some(&root));

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display connection"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Clock + date — every second
    {
        let clock_label = clock_label.clone();
        let date_label = date_label.clone();
        // Initial
        let now = chrono::Local::now();
        clock_label.set_text(&now.format("%H:%M").to_string());
        date_label.set_text(&now.format("%Y-%m-%d").to_string());
        glib::timeout_add_seconds_local(1, move || {
            let now = chrono::Local::now();
            clock_label.set_text(&now.format("%H:%M").to_string());
            date_label.set_text(&now.format("%Y-%m-%d").to_string());
            glib::ControlFlow::Continue
        });
    }

    // Privacy — every 5s
    update_privacy(&privacy_label);
    glib::timeout_add_seconds_local(5, {
        let privacy_label = privacy_label.clone();
        move || { update_privacy(&privacy_label); glib::ControlFlow::Continue }
    });

    // WiFi + volume + battery — every 10s
    update_wifi(&wifi_img);
    update_vol(&vol_img);
    update_battery(&bat_img, &bat_label);
    glib::timeout_add_seconds_local(10, {
        let wifi_img = wifi_img.clone();
        let vol_img = vol_img.clone();
        let bat_img = bat_img.clone();
        let bat_label = bat_label.clone();
        move || {
            update_wifi(&wifi_img);
            update_vol(&vol_img);
            update_battery(&bat_img, &bat_label);
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

fn update_wifi(img: &Image) {
    match wifi_state() {
        Some(true) => img.set_icon_name(Some("network-wireless-signal-good-symbolic")),
        _          => img.set_icon_name(Some("network-wireless-offline-symbolic")),
    }
}

fn update_vol(img: &Image) {
    let muted = std::process::Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.contains("yes"))
        .unwrap_or(false);
    if muted {
        img.set_icon_name(Some("audio-volume-muted-symbolic"));
    } else {
        img.set_icon_name(Some("audio-volume-high-symbolic"));
    }
}

fn update_battery(img: &Image, label: &Label) {
    match battery_state() {
        Some((pct, charging, mins)) => {
            let time = mins.map(|m| {
                if m >= 60 { format!(" · {}h {:02}m", m / 60, m % 60) }
                else { format!(" · {}m", m) }
            }).unwrap_or_default();
            label.set_text(&format!("{pct}%{time}"));
            label.remove_css_class("bat-ok");
            label.remove_css_class("bat-low");
            label.remove_css_class("bat-charge");
            if charging {
                label.add_css_class("bat-charge");
                img.set_icon_name(Some("battery-charging-symbolic"));
            } else if pct < 20 {
                label.add_css_class("bat-low");
                img.set_icon_name(Some("battery-low-symbolic"));
            } else {
                label.add_css_class("bat-ok");
                img.set_icon_name(Some("battery-full-charged-symbolic"));
            }
        }
        None => {
            label.set_text("");
            img.set_icon_name(Some("ac-adapter-symbolic"));
        }
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

fn battery_state() -> Option<(u8, bool, Option<u32>)> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("BAT") { continue; }
        let base = format!("/sys/class/power_supply/{name}");
        let pct: u8 = std::fs::read_to_string(format!("{base}/capacity"))
            .ok()?.trim().parse().ok()?;
        let status = std::fs::read_to_string(format!("{base}/status")).unwrap_or_default();
        let charging = matches!(status.trim(), "Charging" | "Full");
        let time_file = if charging { "time_to_full_now" } else { "time_to_empty_now" };
        let mins = std::fs::read_to_string(format!("{base}/{time_file}"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|s| s / 60)
            .filter(|&m| m > 0 && m < 1440);
        return Some((pct, charging, mins));
    }
    None
}
