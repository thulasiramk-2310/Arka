use adw::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::Cell;
use std::rc::Rc;

mod audio;
use audio::{Action, Volume};

const APP_ID: &str = "org.arka.sound";

fn volume_icon(vol: f64, muted: bool) -> &'static str {
    if muted || vol == 0.0 {
        "audio-volume-muted-symbolic"
    } else if vol < 0.33 {
        "audio-volume-low-symbolic"
    } else if vol < 0.66 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn pct(v: f64) -> String {
    format!("{}%", (v * 100.0).round() as u32)
}

// Only one worker runs at a time. Slider changes are coalesced before dispatch.
struct Controls {
    window: glib::WeakRef<adw::ApplicationWindow>,
    busy: Cell<bool>,
    rendering: Cell<bool>,
    pending_volume: Cell<Option<f64>>,
    output: Cell<Option<Volume>>,
    input: Cell<Option<Volume>>,
    vol_icon: gtk4::Image,
    vol_pct: gtk4::Label,
    vol_scale: gtk4::Scale,
    mute_btn: gtk4::Button,
    sink_label: gtk4::Label,
    mic_icon: gtk4::Image,
    mic_label: gtk4::Label,
    mic_btn: gtk4::Button,
    status: gtk4::Label,
}

impl Controls {
    fn refresh(self: &Rc<Self>, action: Action) {
        if self.busy.replace(true) {
            return;
        }
        self.mute_btn.set_sensitive(false);
        self.mic_btn.set_sensitive(false);
        self.vol_scale.set_sensitive(false);
        // Do not leave an old muted indicator visible while verification is pending.
        self.vol_pct.set_label("Checking…");
        self.vol_icon
            .set_icon_name(Some("dialog-question-symbolic"));
        self.mic_icon
            .set_icon_name(Some("dialog-question-symbolic"));
        self.mute_btn.set_label("Checking…");
        self.mic_btn.set_label("Checking…");
        self.mute_btn.remove_css_class("muted");
        self.mic_btn.remove_css_class("muted");
        let controls = self.clone();
        glib::spawn_future_local(async move {
            let result = gtk4::gio::spawn_blocking(move || audio::refresh(action)).await;
            let c = controls;
            c.rendering.set(true);
            let snapshot = result.unwrap_or_else(|_| audio::Snapshot {
                output: Err("Audio worker failed".into()),
                input: Err("Audio worker failed".into()),
                error: Some("Could not read audio state".into()),
            });
            c.output
                .set(snapshot.output.as_ref().ok().map(|d| d.volume));
            c.input.set(snapshot.input.as_ref().ok().map(|d| d.volume));
            match snapshot.output {
                Ok(device) => {
                    let v = device.volume;
                    c.vol_icon
                        .set_icon_name(Some(volume_icon(v.level, v.muted)));
                    c.vol_pct.set_label(&if v.muted {
                        "Muted".into()
                    } else {
                        pct(v.level)
                    });
                    c.vol_scale.set_value(v.level.min(1.0));
                    c.vol_scale.set_sensitive(true);
                    c.mute_btn.set_sensitive(true);
                    c.mute_btn
                        .set_label(if v.muted { "Unmute" } else { "Mute" });
                    if v.muted {
                        c.mute_btn.add_css_class("muted");
                    }
                    c.sink_label.set_label(&format!("Output: {}", device.name));
                }
                Err(error) => {
                    c.vol_pct.set_label("Unknown");
                    c.mute_btn.set_label("Unavailable");
                    c.sink_label.set_label(&error);
                }
            }
            match snapshot.input {
                Ok(device) => {
                    let muted = device.volume.muted;
                    c.mic_icon.set_icon_name(Some(if muted {
                        "microphone-sensitivity-muted-symbolic"
                    } else {
                        "microphone-sensitivity-high-symbolic"
                    }));
                    c.mic_label.set_label(&device.name);
                    c.mic_btn.set_sensitive(true);
                    c.mic_btn.set_label(if muted { "Unmute" } else { "Mute" });
                    if muted {
                        c.mic_btn.add_css_class("muted");
                    }
                }
                Err(error) => {
                    c.mic_label.set_label(&format!("State unknown: {error}"));
                    c.mic_btn.set_label("Unavailable");
                }
            }
            if let Some(error) = snapshot.error {
                c.status.set_label(&error);
            } else if !matches!(action, Action::Refresh) {
                c.status
                    .set_label("Applies to the default audio devices only.");
            }
            c.rendering.set(false);
            c.busy.set(false);
        });
    }
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    arka_shell_common::app::activate_once(&app, build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    // Follow the desktop look (Light/Dark) instead of forcing dark.
    arka_shell_common::theme::follow_look();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .sound-window {
        background-color: @bg_base;
        border: 1px solid @border_ui;
        border-radius: 14px;
    }
    .vol-icon { color: @text_hi; }
    .vol-pct  { color: @text_hi; font-size: 18px; font-weight: 600; min-width: 52px; }
    .device-label { color: @text_lo; font-size: 12px; }
    .section-title { color: @text_lo; font-size: 11px; font-weight: 600; letter-spacing: 0.5px; }
    .mute-btn {
        background: transparent;
        border: 1px solid @border_ui;
        border-radius: 999px;
        color: @text_hi;
        padding: 4px 12px;
        font-size: 12px;
    }
    .mute-btn:hover { background: alpha(@text_hi, 0.06); }
    .mute-btn.muted { color: @danger; border-color: alpha(@danger, 0.5); }
    scale trough { background-color: @bg_sunken; border-radius: 4px; min-height: 4px; }
    scale highlight { background-color: @text_hi; border-radius: 4px; }
    scale slider { background-color: @bg_base; border: 1px solid @border_emph; border-radius: 50%; min-width: 16px; min-height: 16px; }
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
    let vol_title = gtk4::Label::new(Some("Volume"));
    vol_title.add_css_class("section-title");
    vol_title.set_halign(gtk4::Align::Start);

    let vol = 0.0;
    let muted = false;

    // Icon + percentage row
    let vol_top = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

    let vol_icon = gtk4::Image::from_icon_name(volume_icon(vol, muted));
    vol_icon.set_pixel_size(24);
    vol_icon.add_css_class("vol-icon");

    let vol_pct = gtk4::Label::new(Some(&pct(vol)));
    vol_pct.add_css_class("vol-pct");

    let mute_btn = gtk4::Button::with_label(if muted { "Unmute" } else { "Mute" });
    mute_btn.add_css_class("mute-btn");
    if muted {
        mute_btn.add_css_class("muted");
    }
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
    let sink_name = "Checking…";
    let sink_label = gtk4::Label::new(Some(&format!("Output: {}", sink_name)));
    sink_label.add_css_class("device-label");
    sink_label.set_halign(gtk4::Align::Start);
    sink_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    sink_label.set_max_width_chars(34);

    // ── Microphone section ────────────────────────────────────────────────────
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);

    let mic_title = gtk4::Label::new(Some("Microphone"));
    mic_title.add_css_class("section-title");
    mic_title.set_halign(gtk4::Align::Start);

    let mic_muted = false;

    let mic_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let mic_icon = gtk4::Image::from_icon_name(if mic_muted {
        "microphone-sensitivity-muted-symbolic"
    } else {
        "microphone-sensitivity-high-symbolic"
    });
    mic_icon.set_pixel_size(20);
    mic_icon.add_css_class("vol-icon");

    let src_name = "Checking…";
    let mic_label = gtk4::Label::new(Some(&src_name));
    mic_label.add_css_class("device-label");
    mic_label.set_hexpand(true);
    mic_label.set_halign(gtk4::Align::Start);
    mic_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    mic_label.set_max_width_chars(22);

    let mic_btn = gtk4::Button::with_label(if mic_muted { "Unmute" } else { "Mute" });
    mic_btn.add_css_class("mute-btn");
    if mic_muted {
        mic_btn.add_css_class("muted");
    }

    mic_row.append(&mic_icon);
    mic_row.append(&mic_label);
    mic_row.append(&mic_btn);

    let status = gtk4::Label::new(Some("Applies to the default audio devices only."));
    status.set_wrap(true);
    status.set_max_width_chars(34);
    status.add_css_class("device-label");
    let controls = Rc::new(Controls {
        window: window.downgrade(),
        busy: Cell::new(false),
        rendering: Cell::new(false),
        pending_volume: Cell::new(None),
        output: Cell::new(None),
        input: Cell::new(None),
        vol_icon: vol_icon.clone(),
        vol_pct: vol_pct.clone(),
        vol_scale: vol_scale.clone(),
        mute_btn: mute_btn.clone(),
        sink_label: sink_label.clone(),
        mic_icon: mic_icon.clone(),
        mic_label: mic_label.clone(),
        mic_btn: mic_btn.clone(),
        status: status.clone(),
    });
    let c = Rc::downgrade(&controls);
    vol_scale.connect_value_changed(move |scale| {
        if let Some(c) = c.upgrade() {
            if !c.rendering.get() {
                c.pending_volume.set(Some(scale.value()));
            }
        }
    });
    let c = Rc::downgrade(&controls);
    mute_btn.connect_clicked(move |_| {
        if let Some(c) = c.upgrade() {
            if let Some(v) = c.output.get() {
                c.refresh(Action::MuteOutput(!v.muted));
            }
        }
    });
    let c = Rc::downgrade(&controls);
    mic_btn.connect_clicked(move |_| {
        if let Some(c) = c.upgrade() {
            if let Some(v) = c.input.get() {
                c.refresh(Action::MuteInput(!v.muted));
            }
        }
    });
    let c = controls.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        if c.window.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        if !c.busy.get() {
            if let Some(volume) = c.pending_volume.take() {
                c.refresh(Action::Volume(volume));
            }
        }
        glib::ControlFlow::Continue
    });
    let c = controls.clone();
    glib::timeout_add_seconds_local(5, move || {
        if c.window.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        if !c.busy.get() && c.pending_volume.get().is_none() {
            c.refresh(Action::Refresh);
        }
        glib::ControlFlow::Continue
    });
    controls.refresh(Action::Refresh);

    inner.append(&vol_title);
    inner.append(&vol_top);
    inner.append(&vol_scale);
    inner.append(&sink_label);
    inner.append(&sep);
    inner.append(&mic_title);
    inner.append(&mic_row);
    inner.append(&status);

    root.append(&inner);
    window.set_content(Some(&root));

    // Esc closes
    let ctl = gtk4::EventControllerKey::new();
    let w = window.downgrade();
    ctl.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape {
            if let Some(w) = w.upgrade() {
                w.close();
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(ctl);

    window.present();
    arka_shell_common::theme::preview_shot(&window);
}
