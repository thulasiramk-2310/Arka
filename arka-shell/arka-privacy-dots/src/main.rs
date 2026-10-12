//! arka-privacy-dots: a small indicator, above every window, that names the
//! program using the microphone, camera or screen.
//!
//! It only reads: `pw-dump --monitor` says when the audio and video graph
//! changes, a fresh `pw-dump` gives the picture, and arka-sensors turns that
//! into "who is capturing". Nothing is written, logged or sent anywhere.
//!
//! What it can honestly say, and what it cannot, is in
//! docs/ARKA-PRIVACY-DOTS-PLAN.md. Names marked "reported by the app" come from
//! the app itself; the dot is reliable either way, because it appears only when
//! an active capture stream exists.

use arka_sensors::{describe, find_uses, EventFilter, Kind, Use};
use gtk4::prelude::*;
use gtk4::{gdk, glib};
use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// The design system's "attention" colour (docs: arka-design-system, warn).
const CSS: &str = "
window.dots { background: transparent; }
.pill { background-color: #17191d; border: 1px solid rgba(255,255,255,0.14); border-radius: 999px; padding: 7px 10px; }
.toast { color: #f5f7fa; font-family: 'Inter', 'Noto Sans', sans-serif; font-size: 12px; padding-left: 4px; padding-right: 4px; }
";
const AMBER: (f64, f64, f64) = (0xf5 as f64 / 255.0, 0xa6 as f64 / 255.0, 0x23 as f64 / 255.0);
const TOAST_SECS: u64 = 5;
const DEBOUNCE: Duration = Duration::from_millis(200);
const SAFETY_REFRESH: Duration = Duration::from_secs(15);

fn exe_of(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.to_string_lossy().into_owned())
}

fn snapshot() -> Option<Vec<Use>> {
    let out = Command::new("pw-dump").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let dump: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(find_uses(&dump, &exe_of))
}

/// Watch the graph on background threads; send a fresh list of captures whenever it may have changed.
fn watch(tx: mpsc::Sender<Vec<Use>>) {
    let last_event: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(Some(Instant::now())));
    {
        let last_event = last_event.clone();
        std::thread::spawn(move || loop {
            // `pw-dump --monitor` prints on every change. If it ends (the audio
            // system restarted), start it again after a pause.
            if let Ok(mut child) = Command::new("pw-dump").arg("--monitor").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
                if let Some(out) = child.stdout.take() {
                    // Only changes that can alter who is capturing count; see EventFilter.
                    let mut filter = EventFilter::default();
                    for line in BufReader::new(out).lines().map_while(Result::ok) {
                        if filter.push_line(&line) {
                            *last_event.lock().unwrap() = Some(Instant::now());
                        }
                    }
                }
                let _ = child.wait();
            }
            std::thread::sleep(Duration::from_secs(3));
        });
    }
    std::thread::spawn(move || {
        let mut last_run = Instant::now() - SAFETY_REFRESH;
        let mut last_sent: Option<Vec<Use>> = None;
        // ARKA_DOTS_DEBUG=1 prints how often the graph is re-read (for tuning idle cost).
        let debug = std::env::var_os("ARKA_DOTS_DEBUG").is_some();
        let mut count = 0u64;
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let due = {
                let mut ev = last_event.lock().unwrap();
                match *ev {
                    Some(t) if t.elapsed() >= DEBOUNCE => {
                        *ev = None;
                        true
                    }
                    _ => last_run.elapsed() >= SAFETY_REFRESH,
                }
            };
            if !due {
                continue;
            }
            last_run = Instant::now();
            let started = Instant::now();
            let result = snapshot();
            if debug {
                count += 1;
                eprintln!("snapshot #{count} took {:?}", started.elapsed());
            }
            if let Some(uses) = result {
                if last_sent.as_ref() != Some(&uses) {
                    last_sent = Some(uses.clone());
                    if tx.send(uses).is_err() {
                        return;
                    }
                }
            }
        }
    });
}

fn dot(kind: Kind) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(12);
    area.set_content_height(12);
    area.set_valign(gtk4::Align::Center);
    area.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgb(AMBER.0, AMBER.1, AMBER.2);
        let (cx, cy, r) = (w as f64 / 2.0, h as f64 / 2.0, w.min(h) as f64 / 2.0 - 1.0);
        match kind {
            // Three shapes so the kinds are told apart without colour:
            // microphone = disc, camera = rounded square, screen = ring.
            Kind::Microphone => {
                cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
            Kind::Camera => {
                let s = r * 2.0 - 1.0;
                let (x, y, rad) = (cx - s / 2.0, cy - s / 2.0, 3.0);
                cr.new_sub_path();
                cr.arc(x + s - rad, y + rad, rad, -std::f64::consts::FRAC_PI_2, 0.0);
                cr.arc(x + s - rad, y + s - rad, rad, 0.0, std::f64::consts::FRAC_PI_2);
                cr.arc(x + rad, y + s - rad, rad, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
                cr.arc(x + rad, y + rad, rad, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
                cr.close_path();
                let _ = cr.fill();
            }
            Kind::Screen => {
                cr.set_line_width(2.0);
                cr.arc(cx, cy, r - 0.5, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
            }
        }
    });
    area
}

struct Ui {
    window: gtk4::Window,
    dots: gtk4::Box,
    toast: gtk4::Label,
    shown: Vec<Use>,
    toast_token: u32,
}

fn sentence(u: &Use) -> String {
    let doing = match u.kind {
        Kind::Microphone => "using the microphone",
        Kind::Camera => "using the camera",
        Kind::Screen => "capturing the screen",
    };
    // A false name must not read like a fact, so say who is vouching for it.
    match u.trust {
        arka_sensors::Trust::Verified => format!("{} is {doing}", u.name),
        arka_sensors::Trust::Reported => format!("{} says it is {doing}", u.name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arka_sensors::Trust;

    fn used(kind: Kind, trust: Trust) -> Use {
        Use { kind, name: "app".into(), trust, claimed: "app".into(), pid: None }
    }

    #[test]
    fn a_verified_name_is_stated_as_fact() {
        assert_eq!(sentence(&used(Kind::Microphone, Trust::Verified)), "app is using the microphone");
        assert_eq!(sentence(&used(Kind::Camera, Trust::Verified)), "app is using the camera");
        assert_eq!(sentence(&used(Kind::Screen, Trust::Verified)), "app is capturing the screen");
    }

    #[test]
    fn a_reported_name_says_who_is_vouching_for_it() {
        assert_eq!(sentence(&used(Kind::Microphone, Trust::Reported)), "app says it is using the microphone");
        assert_eq!(sentence(&used(Kind::Screen, Trust::Reported)), "app says it is capturing the screen");
    }
}

fn apply(ui: &Rc<RefCell<Ui>>, uses: Vec<Use>) {
    let mut s = ui.borrow_mut();
    let previous = std::mem::replace(&mut s.shown, uses.clone());
    while let Some(child) = s.dots.first_child() {
        s.dots.remove(&child);
    }
    if uses.is_empty() {
        s.toast.set_visible(false);
        s.window.set_visible(false);
        return;
    }
    for kind in [Kind::Microphone, Kind::Camera, Kind::Screen] {
        if !uses.iter().any(|u| u.kind == kind) {
            continue;
        }
        let d = dot(kind);
        let text = describe(kind, &uses);
        d.set_tooltip_text(Some(&text));
        d.update_property(&[gtk4::accessible::Property::Label(&text)]);
        s.dots.append(&d);
    }
    // Say it out loud for a few seconds when something new starts.
    if let Some(new) = uses.iter().find(|u| !previous.contains(u)) {
        s.toast.set_text(&sentence(new));
        s.toast.set_visible(true);
        s.toast_token = s.toast_token.wrapping_add(1);
        let (token, weak) = (s.toast_token, Rc::downgrade(ui));
        glib::timeout_add_local_once(Duration::from_secs(TOAST_SECS), move || {
            if let Some(ui) = weak.upgrade() {
                let u = ui.borrow();
                if u.toast_token == token {
                    u.toast.set_visible(false);
                }
            }
        });
    }
    s.window.set_visible(true);
}

fn build(app: &gtk4::Application) {
    if let Some(display) = gdk::Display::default() {
        let css = gtk4::CssProvider::new();
        css.load_from_data(CSS);
        gtk4::style_context_add_provider_for_display(&display, &css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    let window = gtk4::Window::builder().application(app).title("ArkaOS privacy dots").decorated(false).resizable(false).focusable(false).build();
    window.add_css_class("dots");
    #[cfg(feature = "layer-shell")]
    {
        use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
        window.init_layer_shell();
        window.set_layer(Layer::Overlay); // above full-screen windows too
        window.set_namespace("arka-privacy-dots");
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Top, 44); // just below the 34 px top bar
        window.set_margin(Edge::Right, 16);
    }
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.add_css_class("pill");
    let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let toast = gtk4::Label::new(None);
    toast.add_css_class("toast");
    toast.set_visible(false);
    row.append(&dots);
    row.append(&toast);
    window.set_child(Some(&row));

    let ui = Rc::new(RefCell::new(Ui { window, dots, toast, shown: Vec::new(), toast_token: 0 }));
    let (tx, rx) = mpsc::channel::<Vec<Use>>();
    watch(tx);
    let ui2 = ui.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        while let Ok(uses) = rx.try_recv() {
            apply(&ui2, uses);
        }
        glib::ControlFlow::Continue
    });
}

fn main() -> glib::ExitCode {
    let app = gtk4::Application::builder().application_id("org.arka.privacydots").build();
    // No window is shown until something is captured, so keep the app alive.
    std::mem::forget(app.hold());
    app.connect_activate(build);
    app.run()
}
