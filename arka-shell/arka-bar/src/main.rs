use arka_shell_common::{theme, DnsStatus};
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CenterBox, Image, Label, Orientation,
    Scale,
};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

const APP_ID: &str = "org.arka.bar";

const STYLE: &str = "
window { background: transparent; }

.arka-bar {
    background-color: @bg_base;
    border-bottom: 1px solid @border_sub;
    padding: 0 12px;
    min-height: 36px;
}

/* Logo */
.logo-btn {
    background: transparent; border: none; box-shadow: none;
    border-radius: 6px; padding: 4px 8px;
}
.logo-btn:hover { background-color: @bg_overlay; }
.logo-mark {
    background-color: @accent;
    border-radius: 6px;
    min-width: 20px; min-height: 20px;
    padding: 0 5px;
}
.logo-mark label { font-size: 12px; font-weight: 800; color: @bg_base; }
.logo-name { font-size: 13px; font-weight: 600; color: @text_hi; margin-left: 8px; }

/* Active app name */
.app-name { font-size: 13px; font-weight: 500; color: @text_lo; margin: 0 0 0 8px; }

/* Center clock */
.clock-center { font-size: 13px; font-weight: 500; color: @text_hi; }

/* Privacy badge */
.privacy-badge {
    font-size: 11px; font-weight: 600; color: @accent;
    background-color: alpha(@accent, 0.10);
    border: 1px solid alpha(@accent, 0.20);
    border-radius: 6px;
    padding: 3px 10px;
    margin: 0 4px;
}
.privacy-badge.warn {
    color: @warn;
    background-color: alpha(@warn, 0.10);
    border-color: alpha(@warn, 0.25);
}

/* Status cluster */
.status-cluster {
    padding: 4px 10px; border-radius: 6px;
    background: transparent;
}
.status-cluster:hover { background-color: @bg_overlay; }
.bat-ok     { color: @text_lo; font-size: 12px; margin-left: 4px; }
.bat-low    { color: @warn; font-size: 12px; margin-left: 4px; }
.bat-charge { color: @info; font-size: 12px; margin-left: 4px; }
image { margin: 0 4px; color: @text_lo; }

/* Power button */
.power-btn-bar {
    background: transparent; border: none; box-shadow: none;
    border-radius: 6px; padding: 2px 6px;
    color: @text_lo;
}
.power-btn-bar:hover { background-color: alpha(@danger, 0.10); color: @danger; }
.power-btn-bar image { color: @text_lo; }
.power-btn-bar:hover image { color: @danger; }

/* Quick Settings popover */
.qs-popover { padding: 16px; min-width: 288px; background-color: @bg_raised; }

.qs-tiles-row { margin-bottom: 8px; }
.qs-tile {
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 10px;
    padding: 12px 12px 8px;
    min-width: 120px;
}
.qs-tile:hover { background-color: @bg_sunken; }
.qs-tile.on {
    background-color: alpha(@accent, 0.10);
    border-color: alpha(@accent, 0.28);
}
.qs-tile image { color: @text_lo; }
.qs-tile.on image { color: @accent; }
.qs-tile-label {
    font-size: 12px; font-weight: 500;
    color: @text_lo; margin-top: 8px;
}
.qs-tile.on .qs-tile-label { color: @accent; }

.qs-slider-label {
    font-size: 12px; color: @text_lo; margin-bottom: 4px;
}
.qs-sep { background-color: @border_sub; margin: 8px 0; }

/* Quick Settings power row */
.qs-power-btn {
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 8px;
    padding: 8px 4px;
    color: @text_lo; font-size: 11px;
    min-width: 62px;
}
.qs-power-btn image { color: @text_lo; }
.qs-power-btn:hover { background-color: @bg_sunken; color: @text_hi; }
.qs-power-btn:hover image { color: @text_hi; }
.qs-power-btn.destruct:hover { background-color: alpha(@danger, 0.12); color: @danger; border-color: alpha(@danger, 0.25); }
.qs-power-btn.destruct:hover image { color: @danger; }

/* Power menu popover */
.power-popover { padding: 8px; min-width: 180px; background-color: @bg_raised; }
.power-action {
    background: transparent; border: none; border-radius: 6px;
    padding: 8px 12px; color: @text_hi; font-size: 13px;
}
.power-action image { color: @text_lo; }
.power-action:hover { background-color: @bg_overlay; }
.power-destruct:hover { background-color: alpha(@danger, 0.12); color: @danger; }
.power-destruct:hover image { color: @danger; }
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
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);
    window.set_exclusive_zone(36);

    theme::install_base();
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display connection"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // ── Left ──────────────────────────────────────────────────────────────
    let left = GtkBox::new(Orientation::Horizontal, 0);

    // Logo: gradient mark + "ARKA" text — matches HTML .tb-logo
    let logo_btn = Button::new();
    logo_btn.add_css_class("logo-btn");
    let logo_inner = GtkBox::new(Orientation::Horizontal, 0);
    logo_inner.set_valign(gtk4::Align::Center);
    let mark_box = GtkBox::new(Orientation::Horizontal, 0);
    mark_box.add_css_class("logo-mark");
    mark_box.set_valign(gtk4::Align::Center);
    let mark_lbl = Label::new(Some("A"));
    mark_box.append(&mark_lbl);
    let name_lbl = Label::new(Some("ARKA"));
    name_lbl.add_css_class("logo-name");
    logo_inner.append(&mark_box);
    logo_inner.append(&name_lbl);
    logo_btn.set_child(Some(&logo_inner));
    logo_btn.connect_clicked(|_| { let _ = std::process::Command::new("arka-launcher").spawn(); });

    // Active app name
    let app_name = Label::new(Some("Desktop"));
    app_name.add_css_class("app-name");

    left.append(&logo_btn);
    left.append(&app_name);

    // ── Center ────────────────────────────────────────────────────────────
    let clock_label = Label::new(Some(""));
    clock_label.add_css_class("clock-center");

    // ── Right ─────────────────────────────────────────────────────────────
    let right = GtkBox::new(Orientation::Horizontal, 4);
    right.set_halign(gtk4::Align::End);

    // Privacy score badge → dashboard
    let privacy_badge = Label::new(Some("Privacy --"));
    privacy_badge.add_css_class("privacy-badge");
    let priv_click = gtk4::GestureClick::new();
    priv_click.connect_released(|_, _, _, _| { let _ = std::process::Command::new("arka-dashboard").spawn(); });
    privacy_badge.add_controller(priv_click);

    // Status cluster → Quick Settings popover
    let status = GtkBox::new(Orientation::Horizontal, 2);
    status.add_css_class("status-cluster");

    let wifi_img = Image::from_icon_name("network-wireless-signal-none-symbolic");
    wifi_img.set_pixel_size(14);
    let bt_img = Image::from_icon_name("bluetooth-symbolic");
    bt_img.set_pixel_size(14);
    let vol_img = Image::from_icon_name("audio-volume-high-symbolic");
    vol_img.set_pixel_size(14);
    let bat_img = Image::from_icon_name("battery-good-symbolic");
    bat_img.set_pixel_size(14);
    let bat_label = Label::new(Some(""));
    bat_label.add_css_class("bat-ok");

    status.append(&wifi_img);
    status.append(&bt_img);
    status.append(&vol_img);
    status.append(&bat_img);
    status.append(&bat_label);

    // ── Quick Settings popover ────────────────────────────────────────────
    let qs_pop = gtk4::Popover::new();
    qs_pop.set_parent(&status);
    qs_pop.set_position(gtk4::PositionType::Bottom);

    let qs_box = GtkBox::new(Orientation::Vertical, 6);
    qs_box.add_css_class("qs-popover");

    // Tile row 1: WiFi + Bluetooth
    let tile_row1 = GtkBox::new(Orientation::Horizontal, 8);
    tile_row1.add_css_class("qs-tiles-row");

    let wifi_tile = make_qs_tile("network-wireless-signal-good-symbolic", "Wi-Fi", wifi_state().unwrap_or(false));
    let wifi_tile_btn = wifi_tile.clone();
    wifi_tile.connect_clicked(move |_| { let _ = std::process::Command::new("arka-wifi").spawn(); });

    let bt_tile = make_qs_tile("bluetooth-symbolic", "Bluetooth", {
        let (powered, _) = bt_state();
        powered
    });
    bt_tile.connect_clicked(move |_| { let _ = std::process::Command::new("arka-bluetooth").spawn(); });

    tile_row1.append(&wifi_tile_btn);
    tile_row1.append(&bt_tile);

    // Tile row 2: Privacy + Focus
    let tile_row2 = GtkBox::new(Orientation::Horizontal, 8);
    tile_row2.add_css_class("qs-tiles-row");

    let priv_tile = make_qs_tile("security-high-symbolic", "Privacy", true);
    priv_tile.connect_clicked(|_| { let _ = std::process::Command::new("arka-dashboard").spawn(); });

    let focus_tile = make_qs_tile("notifications-disabled-symbolic", "Focus", false);
    focus_tile.connect_clicked(|_| { let _ = std::process::Command::new("sh").args(["-c", "notify-send 'Focus Mode' 'Notifications paused'"]).spawn(); });

    tile_row2.append(&priv_tile);
    tile_row2.append(&focus_tile);

    qs_box.append(&tile_row1);
    qs_box.append(&tile_row2);

    // Volume slider
    let sep1 = gtk4::Separator::new(Orientation::Horizontal);
    sep1.add_css_class("qs-sep");
    qs_box.append(&sep1);

    let vol_lbl = Label::new(Some("Volume"));
    vol_lbl.add_css_class("qs-slider-label");
    vol_lbl.set_halign(gtk4::Align::Start);
    qs_box.append(&vol_lbl);

    let vol_slider = Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.05);
    vol_slider.set_value(get_volume());
    vol_slider.set_draw_value(false);
    vol_slider.set_hexpand(true);
    vol_slider.connect_value_changed(|s| {
        let _ = std::process::Command::new("wpctl")
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", s.value())])
            .spawn();
    });
    qs_box.append(&vol_slider);

    // Brightness slider
    let bri_lbl = Label::new(Some("Brightness"));
    bri_lbl.add_css_class("qs-slider-label");
    bri_lbl.set_halign(gtk4::Align::Start);
    qs_box.append(&bri_lbl);

    let bri_slider = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 5.0);
    bri_slider.set_value(get_brightness());
    bri_slider.set_draw_value(false);
    bri_slider.set_hexpand(true);
    bri_slider.connect_value_changed(|s| {
        let _ = std::process::Command::new("brightnessctl")
            .args(["set", &format!("{}%", s.value() as u32)])
            .spawn();
    });
    qs_box.append(&bri_slider);

    // Power row
    let sep2 = gtk4::Separator::new(Orientation::Horizontal);
    sep2.add_css_class("qs-sep");
    qs_box.append(&sep2);

    let power_row = GtkBox::new(Orientation::Horizontal, 6);
    for (icon, label, cmd, destruct) in &[
        ("weather-clear-night-symbolic", "Sleep",    "systemctl suspend",  false),
        ("system-lock-screen-symbolic",  "Lock",     "swaylock",            false),
        ("view-refresh-symbolic",        "Restart",  "systemctl reboot",   false),
        ("system-shutdown-symbolic",     "Shutdown", "systemctl poweroff",  true),
    ] {
        let btn = Button::new();
        btn.add_css_class("qs-power-btn");
        if *destruct { btn.add_css_class("destruct"); }
        let vb = GtkBox::new(Orientation::Vertical, 4);
        vb.set_halign(gtk4::Align::Center);
        let img = Image::from_icon_name(icon);
        img.set_pixel_size(16);
        vb.append(&img);
        vb.append(&Label::new(Some(label)));
        btn.set_child(Some(&vb));
        let c = cmd.to_string();
        let pop = qs_pop.clone();
        btn.connect_clicked(move |_| {
            pop.popdown();
            let _ = std::process::Command::new("sh").args(["-c", &c]).spawn();
        });
        power_row.append(&btn);
    }
    qs_box.append(&power_row);

    qs_pop.set_child(Some(&qs_box));

    let qs_pop2 = qs_pop.clone();
    let status_click = gtk4::GestureClick::new();
    status_click.connect_released(move |_, _, _, _| { qs_pop2.popup(); });
    status.add_controller(status_click);

    // Power button (far right)
    let power_btn = Button::new();
    power_btn.add_css_class("power-btn-bar");
    let power_lbl = Image::from_icon_name("system-shutdown-symbolic");
    power_lbl.set_pixel_size(15);
    power_btn.set_child(Some(&power_lbl));
    power_btn.set_tooltip_text(Some("Power"));

    let pow_popover = gtk4::Popover::new();
    let pop_box = GtkBox::new(Orientation::Vertical, 2);
    pop_box.add_css_class("power-popover");
    for (icon, label, cmd, destruct) in &[
        ("system-lock-screen-symbolic",  "Lock Screen", "swaylock",          false),
        ("weather-clear-night-symbolic", "Sleep",       "systemctl suspend", false),
        ("view-refresh-symbolic",        "Restart",     "systemctl reboot",  false),
        ("system-shutdown-symbolic",     "Shutdown",    "systemctl poweroff", true),
    ] {
        let btn = Button::new();
        btn.add_css_class("power-action");
        if *destruct { btn.add_css_class("power-destruct"); }
        let hb = GtkBox::new(Orientation::Horizontal, 10);
        let img = Image::from_icon_name(icon);
        img.set_pixel_size(15);
        hb.append(&img);
        hb.append(&Label::new(Some(label)));
        btn.set_child(Some(&hb));
        let c = cmd.to_string();
        let pop = pow_popover.clone();
        btn.connect_clicked(move |_| {
            pop.popdown();
            let _ = std::process::Command::new("sh").args(["-c", &c]).spawn();
        });
        pop_box.append(&btn);
    }
    pow_popover.set_child(Some(&pop_box));
    pow_popover.set_parent(&power_btn);
    power_btn.connect_clicked(move |_| pow_popover.popup());

    right.append(&privacy_badge);
    right.append(&status);
    right.append(&power_btn);

    // ── Assemble with CenterBox ──────────────────────────────────────────
    let center_box = CenterBox::new();
    center_box.add_css_class("arka-bar");
    center_box.set_hexpand(true);
    // Pin a definite height so the layer-shell surface commits a real size.
    // Relying on CSS min-height alone left the top-anchored surface with no
    // measured height under the cairo renderer, so it never mapped.
    center_box.set_size_request(-1, 36);
    center_box.set_start_widget(Some(&left));
    center_box.set_center_widget(Some(&clock_label));
    center_box.set_end_widget(Some(&right));

    window.set_child(Some(&center_box));

    // ── Timers ────────────────────────────────────────────────────────────
    update_clock(&clock_label);
    glib::timeout_add_seconds_local(1, {
        let clock_label = clock_label.clone();
        move || { update_clock(&clock_label); glib::ControlFlow::Continue }
    });

    update_active_app(&app_name);
    glib::timeout_add_seconds_local(1, {
        let app_name = app_name.clone();
        move || { update_active_app(&app_name); glib::ControlFlow::Continue }
    });

    update_privacy(&privacy_badge);
    glib::timeout_add_seconds_local(5, {
        let privacy_badge = privacy_badge.clone();
        move || { update_privacy(&privacy_badge); glib::ControlFlow::Continue }
    });

    update_wifi(&wifi_img);
    update_bt(&bt_img);
    update_vol(&vol_img);
    update_battery(&bat_img, &bat_label);
    glib::timeout_add_seconds_local(10, {
        let wifi_img = wifi_img.clone();
        let bt_img = bt_img.clone();
        let vol_img = vol_img.clone();
        let bat_img = bat_img.clone();
        let bat_label = bat_label.clone();
        move || {
            update_wifi(&wifi_img);
            update_bt(&bt_img);
            update_vol(&vol_img);
            update_battery(&bat_img, &bat_label);
            glib::ControlFlow::Continue
        }
    });

    window.present();
}

fn make_qs_tile(icon: &str, label: &str, on: bool) -> Button {
    let btn = Button::new();
    btn.add_css_class("qs-tile");
    if on { btn.add_css_class("on"); }
    btn.set_hexpand(true);
    let vb = GtkBox::new(Orientation::Vertical, 0);
    let img = Image::from_icon_name(icon);
    img.set_pixel_size(18);
    img.set_halign(gtk4::Align::Start);
    vb.append(&img);
    let lbl = Label::new(Some(label));
    lbl.add_css_class("qs-tile-label");
    lbl.set_halign(gtk4::Align::Start);
    vb.append(&lbl);
    btn.set_child(Some(&vb));
    btn
}

/// Run an external command with a hard wall-clock timeout and return its stdout.
/// Guards against tools that hang indefinitely (e.g. `bluetoothctl` with no
/// adapter), which would otherwise block `build_ui` before the bar maps.
fn cmd_stdout(prog: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("timeout")
        .arg("2")
        .arg(prog)
        .args(args)
        .output()
        .ok()?;
    String::from_utf8(out.stdout).ok()
}

fn get_volume() -> f64 {
    let out = cmd_stdout("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .unwrap_or_default();
    out.split_whitespace().nth(1)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.5)
}

fn get_brightness() -> f64 {
    let out = cmd_stdout("brightnessctl", &["get"]).unwrap_or_default();
    let current: f64 = out.trim().parse().unwrap_or(100.0);
    let max_out = cmd_stdout("brightnessctl", &["max"]).unwrap_or_else(|| "100".into());
    let max: f64 = max_out.trim().parse().unwrap_or(100.0);
    if max > 0.0 { (current / max * 100.0).round() } else { 100.0 }
}

fn update_clock(label: &Label) {
    let now = chrono::Local::now();
    label.set_text(&now.format("%a, %b %-d  ·  %H:%M").to_string());
}

fn update_active_app(label: &Label) {
    let title = std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| extract_str_json(&s, "\"class\""))
        .unwrap_or_default();

    let friendly = match title.as_str() {
        "org.arka.dashboard"   => "Privacy Dashboard",
        "org.arka.settings"    => "Arka Settings",
        "org.arka.launcher"    => "Launcher",
        "org.arka.wifi"        => "Wi-Fi",
        "org.arka.bluetooth"   => "Bluetooth",
        "org.arka.sound"       => "Sound",
        "org.arka.capsule"     => "Capsule",
        "org.arka.welcome"     => "Welcome",
        "org.arka.perms"       => "Permissions",
        "org.arka.hotkeys"     => "Hotkeys",
        "org.arka.update"      => "Updates",
        "thunar"               => "Files",
        "foot"                 => "Terminal",
        "firefox"              => "Firefox",
        ""                     => "Desktop",
        other                  => other,
    };
    label.set_text(friendly);
}

fn extract_str_json(s: &str, key: &str) -> Option<String> {
    let pos = s.find(key)?;
    let after = s[pos + key.len()..].trim_start_matches([' ', ':']);
    let after = after.trim_start_matches('"');
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn update_privacy(badge: &Label) {
    match fetch_privacy() {
        Ok((score, dns)) => {
            badge.set_text(&format!("Privacy {score}"));
            if dns == DnsStatus::Encrypted {
                badge.remove_css_class("warn");
            } else {
                badge.add_css_class("warn");
            }
        }
        Err(_) => {
            badge.set_text("Privacy --");
            badge.remove_css_class("warn");
        }
    }
}

fn update_wifi(img: &Image) {
    match wifi_state() {
        Some(true) => img.set_icon_name(Some("network-wireless-signal-good-symbolic")),
        _          => img.set_icon_name(Some("network-wireless-offline-symbolic")),
    }
}

fn update_bt(img: &Image) {
    let (powered, connected) = bt_state();
    if !powered {
        img.set_icon_name(Some("bluetooth-disabled-symbolic"));
        img.set_opacity(0.35);
    } else if connected {
        img.set_icon_name(Some("bluetooth-active-symbolic"));
        img.set_opacity(1.0);
    } else {
        img.set_icon_name(Some("bluetooth-symbolic"));
        img.set_opacity(0.65);
    }
}

fn update_vol(img: &Image) {
    let muted = cmd_stdout("pactl", &["get-sink-mute", "@DEFAULT_SINK@"])
        .map(|s| s.contains("yes"))
        .unwrap_or(false);
    img.set_icon_name(Some(if muted {
        "audio-volume-muted-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }));
}

fn update_battery(img: &Image, label: &Label) {
    match battery_state() {
        Some((pct, charging, mins)) => {
            let time = mins.map(|m| {
                if m >= 60 { format!(" {}h{:02}m", m / 60, m % 60) }
                else       { format!(" {}m", m) }
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
        &conn, "org.arka.arkad", "/org/arka/arkad", "org.arka.arkad",
    )?;
    let score = proxy.get_property::<u8>("PrivacyScore")?;
    let dns = proxy.get_property::<String>("DnsStatus")
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
        if !std::path::Path::new(&format!("/sys/class/net/{name}/wireless")).exists() { continue; }
        let state = std::fs::read_to_string(format!("/sys/class/net/{name}/operstate")).unwrap_or_default();
        return Some(state.trim() == "up");
    }
    None
}

fn bt_state() -> (bool, bool) {
    let out = cmd_stdout("bluetoothctl", &["show"]).unwrap_or_default();
    let powered = out.contains("Powered: yes");
    let connected = if powered {
        cmd_stdout("bluetoothctl", &["devices", "Connected"])
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    } else { false };
    (powered, connected)
}

fn battery_state() -> Option<(u8, bool, Option<u32>)> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("BAT") { continue; }
        let base = format!("/sys/class/power_supply/{name}");
        let pct: u8 = std::fs::read_to_string(format!("{base}/capacity")).ok()?.trim().parse().ok()?;
        let status = std::fs::read_to_string(format!("{base}/status")).unwrap_or_default();
        let charging = matches!(status.trim(), "Charging" | "Full");
        let time_file = if charging { "time_to_full_now" } else { "time_to_empty_now" };
        let mins = std::fs::read_to_string(format!("{base}/{time_file}")).ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|s| s / 60)
            .filter(|&m| m > 0 && m < 1440);
        return Some((pct, charging, mins));
    }
    None
}
