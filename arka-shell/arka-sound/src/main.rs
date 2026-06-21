use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::process::Command;

const APP_ID: &str = "org.arka.sound";

// ── wpctl helpers ────────────────────────────────────────────────────────────

fn get_volume() -> f64 {
    let out = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    // "Volume: 0.65 [MUTED]" or "Volume: 0.65"
    out.split_whitespace().nth(1)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.5)
}

fn is_muted() -> bool {
    let out = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    out.contains("[MUTED]")
}

fn is_mic_muted() -> bool {
    let out = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_SOURCE@"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    out.contains("[MUTED]")
}

fn set_volume(v: f64) {
    let _ = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", v)])
        .spawn();
}

fn toggle_mute() {
    let _ = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .spawn();
}

fn toggle_mic_mute() {
    let _ = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_SOURCE@", "toggle"])
        .spawn();
}

fn volume_icon(vol: f64, muted: bool) -> &'static str {
    if muted || vol == 0.0 { "audio-volume-muted-symbolic" }
    else if vol < 0.33     { "audio-volume-low-symbolic" }
    else if vol < 0.66     { "audio-volume-medium-symbolic" }
    else                   { "audio-volume-high-symbolic" }
}

fn pct(v: f64) -> String {
    format!("{}%", (v * 100.0).round() as u32)
}

// ── sink / source names ──────────────────────────────────────────────────────

fn default_sink_name() -> String {
    let out = Command::new("wpctl")
        .args(["inspect", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    for line in out.lines() {
        if line.contains("node.nick") || line.contains("node.description") {
            if let Some(val) = line.split('"').nth(1) {
                return val.to_string();
            }
        }
    }
    "Speakers".into()
}

fn default_source_name() -> String {
    let out = Command::new("wpctl")
        .args(["inspect", "@DEFAULT_SOURCE@"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    for line in out.lines() {
        if line.contains("node.nick") || line.contains("node.description") {
            if let Some(val) = line.split('"').nth(1) {
                return val.to_string();
            }
        }
    }
    "Microphone".into()
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .sound-window {
        background-color: rgba(8, 12, 26, 0.97);
        border: 1px solid rgba(40, 80, 140, 0.5);
        border-radius: 12px;
    }
    .vol-icon { color: #4fc3f7; }
    .vol-pct  { color: #c0d4e8; font-size: 18px; font-weight: bold; min-width: 52px; }
    .device-label { color: #4a7aa0; font-size: 11px; }
    .section-title { color: #2e5070; font-size: 10px; font-weight: 700; letter-spacing: 1px; }
    .mute-btn {
        background: rgba(20,50,90,0.3);
        border: 1px solid rgba(40,80,140,0.3);
        border-radius: 6px;
        color: #4a7aa0;
        padding: 4px 10px;
        font-size: 11px;
    }
    .mute-btn:hover { background: rgba(30,70,120,0.5); color: #8ab0cc; }
    .mute-btn.muted { color: #f87171; border-color: rgba(150,40,40,0.4); background: rgba(90,20,20,0.3); }
    scale trough { background-color: rgba(20,50,90,0.6); border-radius: 4px; }
    scale highlight { background-color: #4fc3f7; border-radius: 4px; }
    scale slider { background-color: #c8d8f0; border-radius: 50%; min-width: 14px; min-height: 14px; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Sound")
        .default_width(300)
        .default_height(1)
        .resizable(false)
        .decorated(false)
        .build();

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("sound-window");
    root.set_margin_start(0);
    root.set_margin_end(0);

    let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    inner.set_margin_start(16);
    inner.set_margin_end(16);
    inner.set_margin_top(16);
    inner.set_margin_bottom(16);

    // ── Volume section ────────────────────────────────────────────────────────
    let vol_title = gtk4::Label::new(Some("VOLUME"));
    vol_title.add_css_class("section-title");
    vol_title.set_halign(gtk4::Align::Start);

    let vol = get_volume();
    let muted = is_muted();

    // Icon + percentage row
    let vol_top = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

    let vol_icon = gtk4::Image::from_icon_name(volume_icon(vol, muted));
    vol_icon.set_pixel_size(24);
    vol_icon.add_css_class("vol-icon");

    let vol_pct = gtk4::Label::new(Some(&pct(vol)));
    vol_pct.add_css_class("vol-pct");

    let mute_btn = gtk4::Button::with_label(if muted { "Unmute" } else { "Mute" });
    mute_btn.add_css_class("mute-btn");
    if muted { mute_btn.add_css_class("muted"); }
    mute_btn.set_hexpand(true);
    mute_btn.set_halign(gtk4::Align::End);

    vol_top.append(&vol_icon);
    vol_top.append(&vol_pct);
    vol_top.append(&mute_btn);

    // Volume slider
    let vol_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.01);
    vol_scale.set_value(vol);
    vol_scale.set_hexpand(true);
    vol_scale.set_draw_value(false);

    // Output device label
    let sink_name = default_sink_name();
    let sink_label = gtk4::Label::new(Some(&format!("Output: {}", sink_name)));
    sink_label.add_css_class("device-label");
    sink_label.set_halign(gtk4::Align::Start);
    sink_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    sink_label.set_max_width_chars(34);

    // ── Microphone section ────────────────────────────────────────────────────
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);

    let mic_title = gtk4::Label::new(Some("MICROPHONE"));
    mic_title.add_css_class("section-title");
    mic_title.set_halign(gtk4::Align::Start);

    let mic_muted = is_mic_muted();

    let mic_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let mic_icon = gtk4::Image::from_icon_name(
        if mic_muted { "microphone-sensitivity-muted-symbolic" }
        else { "microphone-sensitivity-high-symbolic" }
    );
    mic_icon.set_pixel_size(20);
    mic_icon.add_css_class("vol-icon");

    let src_name = default_source_name();
    let mic_label = gtk4::Label::new(Some(&src_name));
    mic_label.add_css_class("device-label");
    mic_label.set_hexpand(true);
    mic_label.set_halign(gtk4::Align::Start);
    mic_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    mic_label.set_max_width_chars(22);

    let mic_btn = gtk4::Button::with_label(if mic_muted { "Unmute" } else { "Mute" });
    mic_btn.add_css_class("mute-btn");
    if mic_muted { mic_btn.add_css_class("muted"); }

    mic_row.append(&mic_icon);
    mic_row.append(&mic_label);
    mic_row.append(&mic_btn);

    // ── Wire signals ──────────────────────────────────────────────────────────

    // Slider → volume
    let vol_icon_ref = vol_icon.clone();
    let vol_pct_ref = vol_pct.clone();
    vol_scale.connect_value_changed(move |s| {
        let v = s.value();
        set_volume(v);
        vol_icon_ref.set_icon_name(Some(volume_icon(v, false)));
        vol_pct_ref.set_label(&pct(v));
    });

    // Mute button
    let vol_icon_ref2 = vol_icon.clone();
    let vol_pct_ref2 = vol_pct.clone();
    let vol_scale_ref = vol_scale.clone();
    let mute_btn_ref = mute_btn.clone();
    mute_btn.connect_clicked(move |btn| {
        toggle_mute();
        let now_muted = is_muted();
        let v = vol_scale_ref.value();
        vol_icon_ref2.set_icon_name(Some(volume_icon(v, now_muted)));
        if now_muted {
            vol_pct_ref2.set_label("Muted");
            btn.set_label("Unmute");
            btn.add_css_class("muted");
        } else {
            vol_pct_ref2.set_label(&pct(v));
            btn.set_label("Mute");
            btn.remove_css_class("muted");
        }
        mute_btn_ref.set_label(if now_muted { "Unmute" } else { "Mute" });
    });

    // Mic mute button
    let mic_icon_ref = mic_icon.clone();
    mic_btn.connect_clicked(move |btn| {
        toggle_mic_mute();
        let now_muted = is_mic_muted();
        mic_icon_ref.set_icon_name(Some(
            if now_muted { "microphone-sensitivity-muted-symbolic" }
            else { "microphone-sensitivity-high-symbolic" }
        ));
        if now_muted {
            btn.set_label("Unmute");
            btn.add_css_class("muted");
        } else {
            btn.set_label("Mute");
            btn.remove_css_class("muted");
        }
    });

    inner.append(&vol_title);
    inner.append(&vol_top);
    inner.append(&vol_scale);
    inner.append(&sink_label);
    inner.append(&sep);
    inner.append(&mic_title);
    inner.append(&mic_row);

    root.append(&inner);
    window.set_content(Some(&root));

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
