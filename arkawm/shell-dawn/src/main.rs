//! arka-desk: the ArkaOS desktop for ArkaWM. One program draws the home view (wallpaper, the ring
//! of apps around the Arka mark, the menu, the status cards) on the desktop layer, and a slim
//! strip along the top (brand, search, privacy status, date and clock) above the windows.
//!
//! The layout is designed on a 1536 px wide screen and scaled to the real one.
//! Colours, wallpaper, texts, the apps on the ring and the commands behind the menu are settings:
//! see `defaults.conf` (printed by `arka-desk --print-config`) and `config.rs`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;
use gtk::{glib, Align, Orientation};
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

mod config;
mod panels;

use config::Config;

/// The next picture when going back and forth through `len` of them: 0, 1, .. last, .. 1, 0, 1.
fn next_frame(at: usize, forward: bool, len: usize) -> (usize, bool) {
    let forward = if at + 1 >= len { false } else if at == 0 { true } else { forward };
    (if forward { at + 1 } else { at - 1 }, forward)
}

/// Width of the screen the layout was drawn for.
const DESIGN_WIDTH: f64 = 1536.0;

/// Multiply every `px` length in the stylesheet by `k`, so the design scales with the screen.
fn scale_css(css: &str, k: f64) -> String {
    let bytes = css.as_bytes();
    let mut out = String::with_capacity(css.len() + 256);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let after_word = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || matches!(bytes[i - 1], b'#' | b'.' | b'_'));
        if c.is_ascii_digit() && !after_word {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            if css[j..].starts_with("px") {
                let value: f64 = css[i..j].parse().unwrap_or(0.0);
                out.push_str(&format!("{:.2}px", value * k));
                i = j + 2;
            } else {
                out.push_str(&css[i..j]);
                i = j;
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

fn installed(program: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':').any(|dir| Path::new(dir).join(program).is_file())
}

/// The launcher style written from the theme, so the app search looks like the rest of the desktop.
static FUZZEL_STYLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Write a fuzzel style from the theme's colours and font; fuzzel is started with it unless the
/// command names a style of its own.
fn write_fuzzel_style(cfg: &config::Config) {
    let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") else { return };
    let hex = |key: &str, alpha: &str| format!("{}{alpha}", cfg.get(&format!("colors.{key}")).trim_start_matches('#'));
    let font = cfg.get("look.font").split(',').next().unwrap_or("sans-serif").trim().trim_matches('"').to_string();
    let style = format!(
        "[main]\nfont={font}:size=12\nprompt=\"\"\nplaceholder=Search apps\nwidth=36\nlines=10\nhorizontal-pad=22\n\
         vertical-pad=18\ninner-pad=12\nline-height=26\n\n[colors]\nbackground={}\ntext={}\nprompt={}\nplaceholder={}\n\
         input={}\nmatch={}\nselection={}\nselection-text={}\nselection-match={}\ncounter={}\nborder={}\n\n\
         [border]\nwidth=1\nradius=16\nselection-radius=10\n",
        hex("panel", "f0"), hex("text", "ff"), hex("text_dim", "ff"), hex("text_dim", "ff"), hex("text", "ff"),
        hex("accent", "ff"), hex("accent_muted", "d0"), hex("text", "ff"), hex("accent", "ff"), hex("text_dim", "ff"),
        hex("border", "88"),
    );
    let path = format!("{dir}/arka-desk-fuzzel.ini");
    if std::fs::write(&path, style).is_ok() {
        let _ = FUZZEL_STYLE.set(path);
    }
}

/// Start the first of these commands whose program is installed.
fn run(commands: &[String]) {
    for command in commands {
        let program = command.split(' ').next().unwrap_or_default();
        if installed(program) {
            let command = match FUZZEL_STYLE.get() {
                Some(style) if program == "fuzzel" && !command.contains("--config") => format!("{command} --config '{style}'"),
                _ => command.clone(),
            };
            let _ = Command::new("sh").arg("-c").arg(command).spawn();
            return;
        }
    }
    eprintln!("arka-desk: none of {commands:?} is installed");
}

/// Shares of `look.blur` used behind glass: round buttons, the menu and small cards, the
/// information cards. With the default of 8 that is 4, 6 and 8 design pixels.
const GLASS_LEVELS: [f64; 3] = [0.5, 0.75, 1.0];
const ROUND: f64 = 0.5;
const SMALL: f64 = 0.75;
const CARD: f64 = 1.0;

struct Ctx {
    k: f64,
    cfg: Config,
    /// Glass surfaces on the desktop and their corner radius in design pixels (negative: round).
    /// A softened copy of the wallpaper is painted behind each, so they read as frosted glass.
    /// The third number is how much of the glass blur this surface gets (see `GLASS_LEVELS`).
    frost: RefCell<Vec<(gtk::Widget, f64, f64)>>,
    /// Softened copies of the wallpaper, one per blur level, small; stretched when painted.
    /// Only these are blurred. The wallpaper itself is shown as it is.
    soft: RefCell<Vec<(f64, gtk::cairo::ImageSurface)>>,
}

impl Ctx {
    fn frosted(&self, widget: &impl IsA<gtk::Widget>, radius: f64, level: f64) {
        self.frost.borrow_mut().push((widget.clone().upcast(), radius, level));
    }
    fn soft(&self, level: f64) -> Option<gtk::cairo::ImageSurface> {
        self.soft.borrow().iter().find(|(l, _)| *l == level).map(|(_, surface)| surface.clone())
    }
    fn px(&self, v: f64) -> i32 {
        (v * self.k).round() as i32
    }
    fn icon(&self, name: &str, size: f64) -> gtk::Image {
        let image = gtk::Image::from_file(self.cfg.file(&format!("{name}.svg")));
        image.set_pixel_size(self.px(size));
        image
    }
    /// The Arka mark, fitted inside a square of this size.
    fn logo(&self, size: f64) -> gtk::Image {
        let image = gtk::Image::from_file(self.cfg.file("logo.png"));
        image.set_pixel_size(self.px(size));
        image.set_halign(Align::Center);
        image.set_valign(Align::Center);
        image.add_css_class("crystal");
        image
    }
    /// Place a widget at a design position on the overlay.
    fn put(&self, overlay: &gtk::Overlay, widget: &impl IsA<gtk::Widget>, x: f64, y: f64) {
        widget.set_halign(Align::Start);
        widget.set_valign(Align::Start);
        widget.set_margin_start(self.px(x));
        widget.set_margin_top(self.px(y));
        overlay.add_overlay(widget);
    }
    /// Place a widget centred across the screen at a design height.
    fn put_center(&self, overlay: &gtk::Overlay, widget: &impl IsA<gtk::Widget>, y: f64) {
        widget.set_halign(Align::Center);
        widget.set_valign(Align::Start);
        widget.set_margin_top(self.px(y));
        overlay.add_overlay(widget);
    }
    /// Spaced-out capitals with a short line under them.
    fn caps(&self, text: &str, classes: &[&str], center: bool, rule: f64) -> gtk::Box {
        let column = gtk::Box::new(Orientation::Vertical, self.px(12.0));
        let label = gtk::Label::new(Some(text));
        label.add_css_class("caps");
        for class in classes {
            label.add_css_class(class);
        }
        label.set_xalign(if center { 0.5 } else { 0.0 });
        label.set_justify(if center { gtk::Justification::Center } else { gtk::Justification::Left });
        column.append(&label);
        if rule > 0.0 {
            let line = gtk::Box::new(Orientation::Horizontal, 0);
            line.add_css_class("rule");
            line.set_size_request(self.px(rule), 1);
            line.set_halign(if center { Align::Center } else { Align::Start });
            column.append(&line);
        }
        column
    }
    fn node(&self, icon: &str, name: &str, green: bool, commands: Vec<String>) -> gtk::Button {
        let column = gtk::Box::new(Orientation::Vertical, self.px(10.0));
        let disc = gtk::Box::new(Orientation::Horizontal, 0);
        disc.add_css_class("disc");
        if green {
            disc.add_css_class("green");
        }
        disc.set_halign(Align::Center);
        self.frosted(&disc, -1.0, ROUND);
        let image = self.icon(icon, 40.0);
        image.add_css_class("icon");
        image.set_hexpand(true);
        image.set_halign(Align::Center);
        image.set_valign(Align::Center);
        disc.append(&image);
        column.append(&disc);
        let label = gtk::Label::new(Some(name));
        label.add_css_class("name");
        column.append(&label);
        let button = gtk::Button::new();
        button.set_child(Some(&column));
        button.add_css_class("node");
        button.set_size_request(self.px(120.0), -1);
        button.set_tooltip_text(Some(name));
        button.connect_clicked(move |_| run(&commands));
        button
    }
    fn pill(&self, icon: &str, name: &str, width: f64, green: bool) -> gtk::Button {
        let row = gtk::Box::new(Orientation::Horizontal, self.px(22.0));
        row.append(&self.icon(icon, 22.0));
        row.append(&gtk::Label::new(Some(name)));
        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("pill");
        if green {
            button.add_css_class("green");
        }
        button.set_size_request(self.px(width), self.px(44.0));
        self.frosted(&button, 22.0, SMALL);
        button
    }
}

/// A small filled line graph of recent readings (each 0..1).
fn sparkline(ctx: &Ctx, rgb: (f64, f64, f64)) -> (gtk::DrawingArea, Rc<RefCell<VecDeque<f64>>>) {
    let history: Rc<RefCell<VecDeque<f64>>> = Rc::new(RefCell::new(VecDeque::new()));
    let area = gtk::DrawingArea::new();
    area.set_content_width(ctx.px(82.0));
    area.set_content_height(ctx.px(22.0));
    area.set_valign(Align::Center);
    let data = history.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let data = data.borrow();
        let (w, h) = (w as f64, h as f64);
        // a faint floor, so the graph has a place before the first readings arrive
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 0.28);
        cr.rectangle(0.0, h - 1.0, w, 1.0);
        let _ = cr.fill();
        if data.len() < 2 {
            return;
        }
        let step = w / 23.0;
        let start = w - step * (data.len() - 1) as f64;
        let y = |v: f64| h - 2.0 - v.clamp(0.0, 1.0) * (h - 4.0);
        let points: Vec<(f64, f64)> = data.iter().enumerate().map(|(i, v)| (start + step * i as f64, y(*v))).collect();
        // A smooth curve through the readings (Catmull-Rom as Bézier segments). It passes
        // through every reading; only the joins between them are rounded.
        let curve = |cr: &gtk::cairo::Context| {
            cr.move_to(points[0].0, points[0].1);
            for i in 0..points.len() - 1 {
                let p0 = points[i.saturating_sub(1)];
                let (p1, p2) = (points[i], points[i + 1]);
                let p3 = points[(i + 2).min(points.len() - 1)];
                let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, (p1.1 + (p2.1 - p0.1) / 6.0).clamp(1.0, h - 1.0));
                let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, (p2.1 - (p3.1 - p1.1) / 6.0).clamp(1.0, h - 1.0));
                cr.curve_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
            }
        };
        curve(cr);
        cr.line_to(w, h);
        cr.line_to(start, h);
        cr.close_path();
        let fill = gtk::cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
        fill.add_color_stop_rgba(0.0, rgb.0, rgb.1, rgb.2, 0.42);
        fill.add_color_stop_rgba(1.0, rgb.0, rgb.1, rgb.2, 0.0);
        let _ = cr.set_source(&fill);
        let _ = cr.fill();
        curve(cr);
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 1.0);
        cr.set_line_width(1.6);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        let _ = cr.stroke();
    });
    (area, history)
}

/// The six settings shown on the Privacy Status card. `None`: could not be read.
type Privacy = [Option<bool>; 6];
const PRIVACY_NAMES: [&str; 6] =
    ["Firewall", "Encrypted DNS (DoT)", "MAC Randomization", "IPv6 Privacy", "Browser Sandbox", "Automatic Updates"];

fn arkad(property: &str) -> Option<String> {
    let out = Command::new("timeout")
        .args(["3", "busctl", "--system", "--json=short", "get-property", "org.arka.arkad", "/org/arka/arkad", "org.arka.arkad", property])
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Ask the privacy daemon and systemd what is configured. Slow, so it runs on its own thread.
fn read_privacy() -> Privacy {
    let flag = |property: &str| arkad(property).map(|json| json.contains("true"));
    let word = |property: &str, good: &str| arkad(property).map(|json| json.contains(&format!("\"{good}\"")));
    let updates = Command::new("timeout")
        .args(["3", "systemctl", "is-enabled", "bootc-fetch-apply-updates.timer"])
        .output()
        .ok()
        .and_then(|out| match String::from_utf8_lossy(&out.stdout).trim() {
            "enabled" => Some(true),
            "disabled" | "masked" => Some(false),
            _ => None,
        });
    [flag("Firewall"), word("DnsStatus", "Encrypted"), flag("MacRandomization"), flag("Ipv6Privacy"), word("SandboxStatus", "Active"), updates]
}

fn cpu_times() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let fields: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
    let idle = fields.get(3)? + fields.get(4).copied().unwrap_or(0);
    Some((fields.iter().sum(), idle))
}

fn memory_used() -> Option<f64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let field = |name: &str| info.lines().find(|l| l.starts_with(name))?.split_whitespace().nth(1)?.parse::<f64>().ok();
    Some(1.0 - field("MemAvailable:")? / field("MemTotal:")?)
}

/// How full the disk holding the user's files is.
fn disk_used() -> Option<f64> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let out = Command::new("df").args(["--output=pcent", &home]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().nth(1)?.trim().trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)
}

/// Processor temperature in °C, from the processor's own sensor where there is one.
fn temperature() -> Option<f64> {
    let read = |path: PathBuf| std::fs::read_to_string(path).ok()?.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    if let Ok(dir) = std::fs::read_dir("/sys/class/hwmon") {
        for entry in dir.flatten() {
            let name = std::fs::read_to_string(entry.path().join("name")).unwrap_or_default();
            if matches!(name.trim(), "k10temp" | "coretemp" | "zenpower" | "cpu_thermal") {
                if let Some(value) = read(entry.path().join("temp1_input")) {
                    return Some(value);
                }
            }
        }
    }
    read(PathBuf::from("/sys/class/thermal/thermal_zone0/temp"))
}

fn os_version() -> String {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |name: &str| {
        release.lines().find_map(|l| l.strip_prefix(name)).map(|v| v.trim_matches('"').to_string()).filter(|v| !v.is_empty())
    };
    field("IMAGE_VERSION=").or_else(|| field("VERSION_ID=")).or_else(|| field("BUILD_ID=")).unwrap_or_else(|| "unknown".into())
}

/// Whether the image-signature policy on this machine asks for signatures. `None`: unreadable.
/// This reads the policy file only; it does not check that the running system was verified.
fn signature_policy() -> Option<bool> {
    let policy = std::fs::read_to_string("/etc/containers/policy.json").ok()?;
    Some(policy.contains("sigstoreSigned") || policy.contains("signedBy"))
}

/// Load a wallpaper at screen width and grade the picture itself: soften, then saturation,
/// contrast and brightness. Nothing is laid over the desktop.
fn graded(path: &Path, width: i32, cfg: &Config, k: f64) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    use gtk::gdk_pixbuf::{Colorspace, InterpType, Pixbuf};
    let mut pixbuf = Pixbuf::from_file_at_scale(path, width, -1, true).ok()?;
    let (w, h) = (pixbuf.width(), pixbuf.height());
    let blur = cfg.number("wallpaper.blur", 0.0).clamp(0.0, 40.0) * k;
    if blur > 0.2 {
        // shrinking and stretching back is a cheap, even blur
        let factor = 1.0 + blur;
        let small = pixbuf.scale_simple(((w as f64 / factor) as i32).max(1), ((h as f64 / factor) as i32).max(1), InterpType::Bilinear)?;
        pixbuf = small.scale_simple(w, h, InterpType::Bilinear)?;
    }
    let tint = config::rgb(cfg.get("wallpaper.haze_color")).unwrap_or((0.8, 0.8, 0.9));
    let grade = Grade {
        brightness: cfg.number("wallpaper.brightness", 1.0).clamp(0.0, 3.0) as f32,
        contrast: cfg.number("wallpaper.contrast", 1.0).clamp(0.0, 3.0) as f32,
        saturation: cfg.number("wallpaper.saturation", 1.0).clamp(0.0, 3.0) as f32,
        haze: cfg.number("wallpaper.haze", 0.0).clamp(0.0, 1.0) as f32,
        tint: ((tint.0 * 255.0) as f32, (tint.1 * 255.0) as f32, (tint.2 * 255.0) as f32),
    };
    if grade != Grade::NONE {
        let (stride, channels) = (pixbuf.rowstride() as usize, pixbuf.n_channels() as usize);
        let mut bytes = pixbuf.read_pixel_bytes().to_vec();
        for row in bytes.chunks_mut(stride).take(h as usize) {
            for pixel in row.chunks_mut(channels).take(w as usize) {
                if let [r, g, b, ..] = pixel {
                    (*r, *g, *b) = grade.apply(*r, *g, *b);
                }
            }
        }
        pixbuf = Pixbuf::from_bytes(&glib::Bytes::from_owned(bytes), Colorspace::Rgb, pixbuf.has_alpha(), 8, w, h, stride as i32);
    }
    Some(pixbuf)
}

/// A small, soft copy of a wallpaper for painting behind glass. `blur` is in screen pixels.
fn softened(pixbuf: &gtk::gdk_pixbuf::Pixbuf, blur: f64) -> Option<gtk::cairo::ImageSurface> {
    use gtk::gdk::prelude::GdkCairoContextExt;
    // shrinking by about the blur radius and stretching back spreads each point that far
    let factor = blur.max(1.0);
    let (w, h) = (((pixbuf.width() as f64 / factor) as i32).max(8), ((pixbuf.height() as f64 / factor) as i32).max(8));
    let small = pixbuf.scale_simple(w, h, gtk::gdk_pixbuf::InterpType::Bilinear)?;
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, w, h).ok()?;
    let cr = gtk::cairo::Context::new(&surface).ok()?;
    cr.set_source_pixbuf(&small, 0.0, 0.0);
    cr.paint().ok()?;
    drop(cr);
    Some(surface)
}

#[derive(PartialEq, Clone, Copy)]
struct Grade {
    brightness: f32,
    contrast: f32,
    saturation: f32,
    /// How far every colour is pulled towards `tint`: morning haze. 0 is none.
    haze: f32,
    tint: (f32, f32, f32),
}

impl Grade {
    const NONE: Self = Self { brightness: 1.0, contrast: 1.0, saturation: 1.0, haze: 0.0, tint: (0.0, 0.0, 0.0) };
    /// Saturation around the pixel's own lightness, contrast around mid grey, then brightness.
    fn apply(&self, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        let (r, g, b) = (r as f32, g as f32, b as f32);
        let light = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let one = |c: f32, tint: f32| {
            let graded = (((light + (c - light) * self.saturation) - 127.5) * self.contrast + 127.5) * self.brightness;
            (graded + (tint - graded) * self.haze).clamp(0.0, 255.0) as u8
        };
        (one(r, self.tint.0), one(g, self.tint.1), one(b, self.tint.2))
    }
}

struct Live {
    pill_dot: gtk::Box,
    pill_text: gtk::Label,
    date: gtk::Label,
    clock: gtk::Label,
    count: gtk::Label,
    privacy_rows: Vec<(gtk::Image, gtk::Label)>,
    checked: gtk::Label,
    health: Vec<(gtk::Label, gtk::DrawingArea, Rc<RefCell<VecDeque<f64>>>)>,
    updates: gtk::Label,
    signature: gtk::Label,
}

fn layer_window(app: &gtk::Application, class: &str, layer: Layer) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::new(app);
    window.add_css_class(class);
    window.init_layer_shell();
    window.set_layer(layer);
    window.set_namespace(&format!("arka-{class}"));
    for edge in [Edge::Top, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window
}

fn build(app: &gtk::Application) {
    let cfg = Config::load();
    let ring_rgb = {
        // the ring is the accent, lifted towards white so it reads as light
        let (r, g, b) = cfg.color("accent");
        (0.5 + r * 0.5, 0.5 + g * 0.5, 0.5 + b * 0.5)
    };
    let (accent_rgb, ok_rgb) = (cfg.color("accent"), cfg.color("ok"));
    let display = gtk::gdk::Display::default().expect("no display");
    let width = display
        .monitors()
        .item(0)
        .and_then(|m| m.downcast::<gtk::gdk::Monitor>().ok())
        .map(|m| m.geometry().width())
        .unwrap_or(1536);
    let height = (width as f64 * 0.625).round() as i32;
    let ctx = Rc::new(Ctx { k: width as f64 / DESIGN_WIDTH, cfg, frost: RefCell::new(Vec::new()), soft: RefCell::new(Vec::new()) });
    let (k, cfg) = (ctx.k, &ctx.cfg);

    let provider = gtk::CssProvider::new();
    let css = config::apply_strengths(include_str!("style.css"), cfg.number("look.glow", 1.0), cfg.number("look.panel_opacity", 1.0));
    provider.load_from_string(&format!("{}{}", cfg.css_colors(), scale_css(&css.replace("__FONT__", cfg.get("look.font")), k)));
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    // Motion can be switched off here or in the system's own setting; either way nothing animates.
    let settings = gtk::Settings::default();
    if !cfg.on("look.motion") {
        if let Some(settings) = &settings {
            settings.set_gtk_enable_animations(false);
        }
    }
    let motion = settings.is_none_or(|settings| settings.is_gtk_enable_animations());

    // ───── The desktop: wallpaper and the home view, under the windows ─────
    let desk = layer_window(app, "desk", Layer::Background);
    desk.set_anchor(Edge::Bottom, true);
    desk.set_exclusive_zone(-1);
    // the keyboard reaches the desktop once it is clicked, so Tab, Enter and Escape work on it
    desk.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    desk.set_default_size(width, height);
    let overlay = gtk::Overlay::new();
    // The glass: one layer above the wallpaper paints its softened copy where each glass
    // surface sits. (Only on the desktop; sheets and the strip sit over windows, and blurring
    // those needs the compositor.)
    let frost = gtk::DrawingArea::new();
    frost.set_can_target(false);
    frost.add_css_class("frost");
    // Frosted glass is a little lighter than what is behind it: a pale veil over the softened
    // picture, only inside the glass (look.frost; 0 leaves the softened picture as it is).
    let veil = cfg.number("look.frost", 0.10).clamp(0.0, 0.6);
    frost.set_draw_func({
        let ctx = ctx.clone();
        move |area, cr, w, h| {
            for (widget, radius, level) in ctx.frost.borrow().iter() {
                if !widget.is_mapped() || widget.has_css_class("shut") {
                    continue;
                }
                let Some(surface) = ctx.soft(*level) else { continue };
                let surface = &surface;
                let scale = (w as f64 / surface.width() as f64).max(h as f64 / surface.height() as f64);
                let (ox, oy) = ((w as f64 - surface.width() as f64 * scale) / 2.0, (h as f64 - surface.height() as f64 * scale) / 2.0);
                let Some(b) = widget.compute_bounds(area) else { continue };
                let (x, y, bw, bh) = (b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64);
                let r = if *radius < 0.0 { bw.min(bh) / 2.0 } else { (radius * ctx.k).min(bw.min(bh) / 2.0) };
                use std::f64::consts::{FRAC_PI_2, PI};
                cr.new_sub_path();
                cr.arc(x + bw - r, y + r, r, -FRAC_PI_2, 0.0);
                cr.arc(x + bw - r, y + bh - r, r, 0.0, FRAC_PI_2);
                cr.arc(x + r, y + bh - r, r, FRAC_PI_2, PI);
                cr.arc(x + r, y + r, r, PI, PI + FRAC_PI_2);
                cr.close_path();
                let _ = cr.save();
                cr.clip();
                cr.translate(ox, oy);
                cr.scale(scale, scale);
                let _ = cr.set_source_surface(surface, 0.0, 0.0);
                let _ = cr.paint();
                cr.identity_matrix();
                cr.set_source_rgba(0.86, 0.90, 0.97, veil);
                let _ = cr.paint();
                let _ = cr.restore();
            }
        }
    });
    let glass_blur = cfg.number("look.blur", 8.0).clamp(0.0, 80.0) * k;
    let picture = {
        let (ctx, frost) = (ctx.clone(), frost.clone());
        Rc::new(move |path: &Path| {
            let picture = match graded(path, width, &ctx.cfg, ctx.k) {
                Some(pixbuf) => {
                    if glass_blur > 0.0 {
                        *ctx.soft.borrow_mut() =
                            GLASS_LEVELS.iter().filter_map(|level| Some((*level, softened(&pixbuf, glass_blur * level)?))).collect();
                        frost.queue_draw();
                    }
                    gtk::Picture::for_paintable(&gtk::gdk::Texture::for_pixbuf(&pixbuf))
                }
                None => gtk::Picture::for_filename(path),
            };
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture
        })
    };
    // A folder of pictures makes a living wallpaper: they fade slowly from the first to the last
    // and back. Otherwise the one still picture. Each picture is graded when it is first shown.
    let own = |key: &str, file: &str| Some(cfg.get(key)).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| cfg.file(file));
    let mut frames: Vec<PathBuf> = std::fs::read_dir(own("wallpaper.frames", "frames"))
        .map(|dir| {
            let is_picture = |p: &PathBuf| p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e, "jpg" | "jpeg" | "png" | "webp"));
            dir.flatten().map(|entry| entry.path()).filter(is_picture).collect()
        })
        .unwrap_or_default();
    frames.sort();
    let living = cfg.on("wallpaper.living") && cfg.get("wallpaper.file").is_empty() && std::env::var_os("ARKA_STILL_WALLPAPER").is_none();
    if frames.len() > 1 && living {
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(if motion { cfg.number("wallpaper.fade", 6000.0).clamp(0.0, 60000.0) as u32 } else { 0 });
        let mut pictures: Vec<Option<gtk::Picture>> = frames.iter().map(|_| None).collect();
        let first = picture(&frames[0]);
        stack.add_child(&first);
        pictures[0] = Some(first);
        overlay.set_child(Some(&stack));
        let (mut at, mut forward) = (0usize, true);
        let (stack, picture) = (stack.clone(), picture.clone());
        glib::timeout_add_seconds_local(cfg.number("wallpaper.every", 30.0).clamp(5.0, 86400.0) as u32, move || {
            (at, forward) = next_frame(at, forward, frames.len());
            // loading the picture again also refreshes the glass behind the cards
            if let Some(old) = pictures[at].take() {
                stack.remove(&old);
            }
            let shown = picture(&frames[at]);
            stack.add_child(&shown);
            stack.set_visible_child(&shown);
            pictures[at] = Some(shown);
            glib::ControlFlow::Continue
        });
    } else {
        overlay.set_child(Some(&picture(&own("wallpaper.file", "wallpaper.jpg"))));
    }

    overlay.add_overlay(&frost);
    // entrance and menu animations move the glass; paint it again once they have settled
    let refrost = {
        let frost = frost.clone();
        Rc::new(move || {
            frost.queue_draw();
            for ms in [120, 360, 800] {
                let frost = frost.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || frost.queue_draw());
            }
        })
    };
    refrost();

    // Texts can be changed or emptied in the settings; an empty one is left out.
    let captions = cfg.on("show.captions");
    let text = |key: &str| Some(cfg.get(&format!("text.{key}"))).filter(|text| !text.is_empty());
    let caption = |key: &str| text(key).filter(|_| captions);

    if let Some(text) = caption("top") {
        ctx.put_center(&overlay, &ctx.caps(text, &[], true, 26.0), 46.0);
    }
    if let Some(text) = caption("left") {
        ctx.put(&overlay, &ctx.caps(text, &["big"], false, 26.0), 36.0, 122.0);
    }
    let quotes = cfg.on("show.quotes");
    if let Some(text) = text("quote_left").filter(|_| quotes) {
        let quote = gtk::Label::new(Some(text));
        quote.add_css_class("glass");
        quote.add_css_class("quote");
        ctx.frosted(&quote, 8.0, SMALL);
        quote.set_xalign(0.0);
        ctx.put(&overlay, &quote, 36.0, 289.0);
    }
    let show_ring = cfg.on("show.ring");

    // The ring and its lights.
    let ring = gtk::DrawingArea::new();
    ring.set_content_width(ctx.px(560.0));
    ring.set_content_height(ctx.px(560.0));
    ring.set_can_target(false);
    ring.set_draw_func(move |_, cr, _, _| {
        use std::f64::consts::TAU;
        cr.scale(k, k);
        let (r, g, b) = ring_rgb;
        for (width, alpha) in [(9.0, 0.06), (5.0, 0.11), (1.4, 0.62)] {
            cr.set_source_rgba(r, g, b, alpha);
            cr.set_line_width(width);
            cr.arc(280.0, 280.0, 224.0, 0.0, TAU);
            let _ = cr.stroke();
        }
        cr.set_source_rgba(r, g, b, 0.38);
        cr.set_line_width(1.0);
        cr.arc(280.0, 280.0, 162.0, 0.0, TAU);
        let _ = cr.stroke();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
        for i in 0..8 {
            let angle = TAU * (i as f64 + 0.5) / 8.0;
            cr.arc(280.0 + 224.0 * angle.cos(), 280.0 + 224.0 * angle.sin(), 2.6, 0.0, TAU);
            let _ = cr.fill();
        }
        for i in 0..4 {
            let angle = TAU * i as f64 / 4.0;
            cr.arc(280.0 + 162.0 * angle.cos(), 280.0 + 162.0 * angle.sin(), 2.0, 0.0, TAU);
            let _ = cr.fill();
        }
    });
    if show_ring {
        ctx.put(&overlay, &ring, 488.0, 96.0);
    }

    let centre = gtk::Box::new(Orientation::Vertical, 0);
    centre.set_size_request(ctx.px(400.0), -1);
    centre.add_css_class("centre");
    // The mark is a control: it brings back the desktop (see below, once the strip exists).
    let mark = gtk::Button::new();
    mark.add_css_class("mark");
    mark.set_child(Some(&ctx.logo(196.0)));
    mark.set_halign(Align::Center);
    mark.set_tooltip_text(Some("Show the desktop"));
    mark.update_property(&[gtk::accessible::Property::Label("Show the desktop")]);
    centre.append(&mark);
    if let Some(text) = text("wordmark") {
        let wordmark = gtk::Label::new(Some(text));
        wordmark.add_css_class("wordmark");
        wordmark.set_margin_top(ctx.px(14.0));
        centre.append(&wordmark);
    }
    if show_ring {
        ctx.put(&overlay, &centre, 568.0, 268.0);
    }

    // The eight places on the ring, clockwise from the top.
    const PLACES: [(f64, f64); 8] =
        [(708.0, 110.0), (866.0, 176.0), (932.0, 334.0), (866.0, 492.0), (708.0, 558.0), (550.0, 492.0), (484.0, 334.0), (550.0, 176.0)];
    if show_ring {
        for (i, ((name, icon, commands, highlight), (x, y))) in cfg.apps().into_iter().zip(PLACES).enumerate() {
            let node = ctx.node(&icon, &name, highlight, commands);
            node.add_css_class(&format!("n{}", i + 1));
            ctx.put(&overlay, &node, x, y);
        }
        if let Some(text) = caption("platform") {
            ctx.put_center(&overlay, &ctx.caps(text, &[], true, 40.0), 724.0);
        }
    }
    write_fuzzel_style(cfg);
    let all_apps = Rc::new(cfg.commands("all_apps"));
    let privacy_app = Rc::new(cfg.commands("privacy"));

    // The menu on the left. Only the round button shows; a click on it opens the menu.
    let mut menu_parts: Vec<gtk::Widget> = Vec::new();
    // The menu's body and the lobe around the round button are one shape with one outline:
    // the button grows into the menu. Drawn here because a style sheet cannot join two shapes.
    let body = gtk::DrawingArea::new();
    body.set_can_target(false);
    body.set_draw_func({
        let ctx = ctx.clone();
        let (raised, panel, border) = (ctx.cfg.color("panel_raised"), ctx.cfg.color("panel"), ctx.cfg.color("border"));
        let strength = ctx.cfg.number("look.panel_opacity", 1.0);
        move |_, cr, w, h| {
            use std::f64::consts::{FRAC_PI_2, PI};
            let k = ctx.k;
            // design pixels: the lobe (centre, radius), the body's box and corners, the fillet
            let (cx, cy, r) = (116.0 * k, 551.0 * k, 62.0 * k);
            let (x0, y0, x1, y1) = (152.0 * k, 422.0 * k, 420.0 * k, 704.0 * k);
            let (tl, tr, br, bl) = (40.0 * k, 60.0 * k, 66.0 * k, 60.0 * k);
            let f = 26.0 * k;
            let fx = x0 - f;
            let reach = ((r + f).powi(2) - (fx - cx).powi(2)).max(0.0).sqrt();
            let (up, low) = (cy - reach, cy + reach);
            cr.new_path();
            cr.arc(x0 + tl, y0 + tl, tl, PI, PI + FRAC_PI_2);
            cr.arc(x1 - tr, y0 + tr, tr, -FRAC_PI_2, 0.0);
            cr.arc(x1 - br, y1 - br, br, 0.0, FRAC_PI_2);
            cr.arc(x0 + bl, y1 - bl, bl, FRAC_PI_2, PI);
            // up the left edge, round the lobe, and back onto the edge, with a soft neck each side
            let toward = |fy: f64| (cy - fy).atan2(cx - fx);
            cr.arc_negative(fx, low, f, 0.0, toward(low));
            cr.arc(cx, cy, r, toward(low) + PI, toward(up) + PI);
            cr.arc_negative(fx, up, f, toward(up), 0.0);
            cr.close_path();
            let _ = cr.save();
            cr.clip_preserve();
            if let Some(surface) = ctx.soft(SMALL).as_ref() {
                let scale = (w as f64 / surface.width() as f64).max(h as f64 / surface.height() as f64);
                let _ = cr.save();
                cr.translate((w as f64 - surface.width() as f64 * scale) / 2.0, (h as f64 - surface.height() as f64 * scale) / 2.0);
                cr.scale(scale, scale);
                let _ = cr.set_source_surface(surface, 0.0, 0.0);
                let _ = cr.paint();
                let _ = cr.restore();
            }
            let tint = gtk::cairo::LinearGradient::new(x0, y0, x1, y1);
            tint.add_color_stop_rgba(0.0, raised.0, raised.1, raised.2, (0.80 * strength).min(1.0));
            tint.add_color_stop_rgba(1.0, panel.0, panel.1, panel.2, (0.90 * strength).min(1.0));
            let _ = cr.set_source(&tint);
            let _ = cr.paint();
            let _ = cr.restore();
            cr.set_source_rgba(border.0, border.1, border.2, 0.5);
            cr.set_line_width(1.2 * k);
            let _ = cr.stroke();
        }
    });
    overlay.add_overlay(&body);
    // shut, only the round button is left: a clean circle
    menu_parts.push(body.upcast());
    let orb = gtk::Button::new();
    orb.add_css_class("orb");
    ctx.frosted(&orb, -1.0, ROUND);
    orb.set_child(Some(&ctx.logo(44.0)));
    orb.set_tooltip_text(Some("Menu"));
    orb.update_property(&[gtk::accessible::Property::Label("Menu")]);
    ctx.put(&overlay, &orb, 74.0, 509.0);
    let stem = gtk::Box::new(Orientation::Vertical, 0);
    stem.add_css_class("rule");
    stem.set_size_request(1, ctx.px(28.0));
    ctx.put(&overlay, &stem, 116.0, 598.0);
    if let Some(text) = text("menu_hint") {
        let open = gtk::Box::new(Orientation::Horizontal, ctx.px(8.0));
        let open_label = gtk::Label::new(Some(text));
        open_label.add_css_class("open");
        open.append(&open_label);
        open.append(&ctx.icon("arrow-right-bold", 14.0));
        ctx.put(&overlay, &open, 58.0, 633.0);
    }

    let slots = [(190.0, 442.0, 180.0), (180.0, 490.0, 182.0), (184.0, 539.0, 180.0), (198.0, 588.0, 182.0), (216.0, 637.0, 184.0)];
    let main_pills: Vec<gtk::Button> = [
        ("squares-four-fill", "Applications", false),
        ("folder-fill", "Places", false),
        ("shield-green", "Privacy", true),
        ("cube-fill", "System", false),
        ("power-bold", "Power", false),
    ]
    .iter()
    .zip(slots)
    .map(|((icon, name, green), (x, y, w))| {
        let pill = ctx.pill(icon, name, w, *green);
        ctx.put(&overlay, &pill, x, y);
        pill
    })
    .collect();
    // "Power" swaps the menu for these, so nothing drastic happens on a single click.
    let power_pills: Vec<gtk::Button> = ["Lock", "Log out", "Restart", "Shut down", "Back"]
        .iter()
        .zip(slots)
        .map(|(name, (x, y, w))| {
            let icon = if *name == "Back" { "arrow-right-bold" } else { "power-bold" };
            let pill = ctx.pill(icon, name, w, false);
            pill.set_visible(false);
            ctx.put(&overlay, &pill, x, y);
            pill
        })
        .collect();
    for (i, pill) in main_pills.iter().chain(&power_pills).enumerate() {
        pill.add_css_class(&format!("m{}", i % 5 + 1));
        menu_parts.push(pill.clone().upcast());
    }
    // Shut, the parts are see-through and take no clicks; the style sheet animates the change.
    let always_open = cfg.get("menu.open") == "always";
    let menu_open = Rc::new(Cell::new(false));
    let set_menu = {
        let (parts, open, refrost) = (menu_parts, menu_open.clone(), refrost.clone());
        Rc::new(move |on: bool| {
            let on = on || always_open;
            open.set(on);
            for part in &parts {
                part.add_css_class("menuitem");
                if on {
                    part.remove_css_class("shut");
                } else {
                    part.add_css_class("shut");
                }
                if part.is::<gtk::Button>() {
                    part.set_can_target(on);
                    part.set_can_focus(on);
                }
            }
            refrost();
        })
    };
    set_menu(false);
    for (pill, key) in main_pills.iter().zip(["all_apps", "places", "privacy", "system"]) {
        let (set_menu, commands) = (set_menu.clone(), cfg.commands(key));
        pill.connect_clicked(move |_| {
            set_menu(false);
            run(&commands);
        });
    }
    let swap = {
        let (main, power, refrost) = (main_pills.clone(), power_pills.clone(), refrost.clone());
        move |show_power: bool| {
            main.iter().for_each(|p| p.set_visible(!show_power));
            power.iter().for_each(|p| p.set_visible(show_power));
            refrost();
        }
    };
    main_pills[4].connect_clicked({
        let swap = swap.clone();
        move |_| swap(true)
    });
    power_pills[4].connect_clicked({
        let swap = swap.clone();
        move |_| swap(false)
    });
    for (pill, key) in power_pills.iter().zip(["lock", "log_out", "restart", "shut_down"]) {
        let (swap, set_menu) = (swap.clone(), set_menu.clone());
        // these are run as written, not looked up: they are shell lines, not program names
        let command = cfg.get(&format!("commands.{key}")).to_string();
        pill.connect_clicked(move |_| {
            swap(false);
            set_menu(false);
            let _ = Command::new("sh").arg("-c").arg(&command).spawn();
        });
    }
    orb.connect_clicked({
        let (swap, set_menu, open) = (swap.clone(), set_menu.clone(), menu_open.clone());
        move |_| {
            swap(false);
            set_menu(!open.get());
        }
    });
    // A click anywhere else on the desktop, or Escape, shuts the menu. The click itself still
    // goes to whatever was under it.
    let outside = gtk::GestureClick::new();
    outside.set_propagation_phase(gtk::PropagationPhase::Capture);
    outside.connect_pressed({
        let (swap, set_menu, open, overlay, orb, pills) =
            (swap.clone(), set_menu.clone(), menu_open.clone(), overlay.clone(), orb.clone(), [main_pills.clone(), power_pills.clone()].concat());
        move |_, _, x, y| {
            if !open.get() {
                return;
            }
            let hit = overlay.pick(x, y, gtk::PickFlags::DEFAULT);
            let on_menu = hit.is_some_and(|hit| hit.is_ancestor(&orb) || hit == orb || pills.iter().any(|pill| hit.is_ancestor(pill) || &hit == pill));
            if !on_menu {
                swap(false);
                set_menu(false);
            }
        }
    });
    overlay.add_controller(outside);
    let escape = gtk::EventControllerKey::new();
    escape.connect_key_pressed({
        let (swap, set_menu, open) = (swap.clone(), set_menu.clone(), menu_open.clone());
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape && open.get() {
                swap(false);
                set_menu(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    desk.add_controller(escape);

    // The cards on the right.
    let aside = gtk::Box::new(Orientation::Vertical, ctx.px(10.0));
    aside.set_size_request(ctx.px(300.0), -1);
    aside.add_css_class("aside");
    let card = |title: &str, gap: f64| {
        let column = gtk::Box::new(Orientation::Vertical, ctx.px(gap));
        column.add_css_class("glass");
        column.add_css_class("card");
        ctx.frosted(&column, 8.0, CARD);
        let head = gtk::Box::new(Orientation::Horizontal, 0);
        let label = gtk::Label::new(Some(title));
        label.add_css_class("title");
        label.set_hexpand(true);
        label.set_xalign(0.0);
        head.append(&label);
        head.set_margin_bottom(ctx.px(4.0));
        column.append(&head);
        (column, head)
    };

    let (privacy_card, privacy_head) = card("Privacy Status", 5.0);
    let count = gtk::Label::new(Some("–"));
    count.add_css_class("title");
    privacy_head.append(&count);
    let mut privacy_rows = Vec::new();
    for name in PRIVACY_NAMES {
        let row = gtk::Box::new(Orientation::Horizontal, ctx.px(14.0));
        row.add_css_class("row");
        let mark = ctx.icon("status-unknown", 21.0);
        row.append(&mark);
        let label = gtk::Label::new(Some(name));
        label.set_hexpand(true);
        label.set_xalign(0.0);
        row.append(&label);
        let status = gtk::Label::new(Some("Checking…"));
        status.add_css_class("dim");
        row.append(&status);
        privacy_card.append(&row);
        privacy_rows.push((mark, status));
    }
    let checked_row = gtk::Box::new(Orientation::Horizontal, ctx.px(12.0));
    checked_row.set_margin_top(ctx.px(4.0));
    checked_row.append(&ctx.icon("arrows-clockwise-bold", 18.0));
    let checked = gtk::Label::new(Some("Not checked yet"));
    checked.add_css_class("dim");
    checked_row.append(&checked);
    privacy_card.append(&checked_row);
    aside.append(&privacy_card);

    let (health_card, _) = card("System Health", 11.0);
    let mut health = Vec::new();
    for (icon, name, rgb) in [
        ("cpu-bold", "CPU", accent_rgb),
        ("memory-bold", "Memory", ok_rgb),
        ("hard-drive-bold", "Disk", accent_rgb),
        ("thermometer-simple-bold", "Temperature", accent_rgb),
    ] {
        let row = gtk::Box::new(Orientation::Horizontal, ctx.px(10.0));
        row.add_css_class("row");
        row.append(&ctx.icon(icon, 24.0));
        let label = gtk::Label::new(Some(name));
        label.set_xalign(0.0);
        label.set_size_request(ctx.px(84.0), -1);
        row.append(&label);
        let value = gtk::Label::new(Some("–"));
        value.set_xalign(1.0);
        value.add_css_class("value");
        value.set_size_request(ctx.px(40.0), -1);
        row.append(&value);
        let (graph, history) = sparkline(&ctx, rgb);
        graph.set_hexpand(true);
        graph.set_halign(Align::End);
        row.append(&graph);
        health_card.append(&row);
        health.push((value, graph, history));
    }
    aside.append(&health_card);

    let (updates_card, _) = card("Updates", 6.0);
    let updates_row = gtk::Box::new(Orientation::Horizontal, ctx.px(14.0));
    updates_row.add_css_class("row");
    updates_row.append(&ctx.icon("arrows-clockwise-bold", 21.0));
    let updates = gtk::Label::new(Some("Checking…"));
    updates.set_xalign(0.0);
    updates_row.append(&updates);
    updates_card.append(&updates_row);
    let version_row = gtk::Box::new(Orientation::Horizontal, 0);
    let version_name = gtk::Label::new(Some("Current version"));
    version_name.add_css_class("dim");
    version_name.set_hexpand(true);
    version_name.set_xalign(0.0);
    version_name.set_margin_start(ctx.px(35.0));
    version_row.append(&version_name);
    let version = gtk::Label::new(Some(&os_version()));
    version.add_css_class("row");
    version_row.append(&version);
    let updates_app = cfg.commands("updates");
    if !updates_app.is_empty() {
        let open = gtk::Button::new();
        open.set_child(Some(&ctx.icon("caret-right-bold", 14.0)));
        open.add_css_class("flat");
        open.add_css_class("chevron");
        open.set_tooltip_text(Some("Open updates"));
        open.update_property(&[gtk::accessible::Property::Label("Open updates")]);
        open.connect_clicked(move |_| run(&updates_app));
        version_row.append(&open);
    }
    updates_card.append(&version_row);
    let signature_row = gtk::Box::new(Orientation::Horizontal, 0);
    let signature_name = gtk::Label::new(Some("Signature policy"));
    signature_name.add_css_class("dim");
    signature_name.set_hexpand(true);
    signature_name.set_xalign(0.0);
    signature_name.set_margin_start(ctx.px(35.0));
    signature_name.set_tooltip_text(Some("Whether this computer's image policy asks for signed system images"));
    signature_row.append(&signature_name);
    let signature = gtk::Label::new(Some("–"));
    signature.add_css_class("row");
    signature_row.append(&signature);
    updates_card.append(&signature_row);
    aside.append(&updates_card);
    if cfg.on("show.cards") {
        ctx.put(&overlay, &aside, 1214.0, 77.0);
    }

    if let Some(text) = text("quote_right").filter(|_| quotes) {
        let quote = gtk::Label::new(Some(text));
        quote.add_css_class("glass");
        quote.add_css_class("quote");
        ctx.frosted(&quote, 8.0, SMALL);
        quote.set_xalign(0.0);
        quote.set_size_request(ctx.px(240.0), -1);
        ctx.put(&overlay, &quote, 1246.0, 663.0);
    }
    if let Some(text) = caption("bottom_left") {
        ctx.put(&overlay, &ctx.caps(text, &["small"], false, 26.0), 36.0, 800.0);
    }
    if let Some(text) = caption("bottom_centre") {
        ctx.put_center(&overlay, &ctx.caps(text, &["small", "italic"], true, 0.0), 826.0);
    }
    if let Some(text) = caption("bottom_right") {
        ctx.put(&overlay, &ctx.caps(text, &["small"], false, 26.0), 1374.0, 826.0);
    }

    desk.set_child(Some(&overlay));
    desk.present();

    // ───── The strip along the top, above the windows ─────
    let strip = layer_window(app, "strip", Layer::Top);
    strip.set_exclusive_zone(ctx.px(56.0));
    strip.set_default_size(width, ctx.px(60.0));
    let bar = gtk::CenterBox::new();
    bar.set_margin_start(ctx.px(34.0));
    bar.set_margin_end(ctx.px(30.0));
    bar.set_margin_top(ctx.px(11.0));
    bar.set_margin_bottom(ctx.px(11.0));
    let brand = gtk::Box::new(Orientation::Horizontal, ctx.px(14.0));
    brand.append(&ctx.logo(36.0));
    if let Some(text) = text("brand") {
        let name = gtk::Label::new(Some(text));
        name.add_css_class("brand");
        brand.append(&name);
    }
    bar.set_start_widget(Some(&brand));

    // Open windows in the middle; notifications, quick settings and clipboard history on the right.
    let shell = panels::Shell::new(app, ctx.clone());
    mark.connect_clicked({
        let (shell, swap, set_menu) = (shell.clone(), swap.clone(), set_menu.clone());
        move |_| {
            swap(false);
            set_menu(false);
            shell.show_desktop();
        }
    });
    if cfg.on("show.tasks") {
        bar.set_center_widget(Some(shell.tasks()));
    }

    let right = gtk::Box::new(Orientation::Horizontal, ctx.px(18.0));
    let round = |icon: &str, tip: &str| {
        let button = gtk::Button::new();
        let image = ctx.icon(icon, 18.0);
        image.set_halign(Align::Center);
        button.set_child(Some(&image));
        button.add_css_class("glass");
        button.add_css_class("round");
        button.set_tooltip_text(Some(tip));
        button.update_property(&[gtk::accessible::Property::Label(tip)]);
        button.set_valign(Align::Center);
        button
    };
    let search = round("magnifying-glass-bold", "Search apps");
    search.connect_clicked({
        let all_apps = all_apps.clone();
        move |_| run(&all_apps)
    });
    right.append(&search);
    let bell = round("bell-bold", "Notifications");
    let me = shell.clone();
    bell.connect_clicked(move |_| me.toggle("notifications"));
    shell.set_bell(&bell);
    right.append(&bell);
    let quick = round("sliders-horizontal-bold", "Quick settings");
    let me = shell.clone();
    quick.connect_clicked(move |_| me.toggle("quick"));
    right.append(&quick);
    let pill = gtk::Button::new();
    pill.add_css_class("glass");
    pill.add_css_class("statuspill");
    pill.set_valign(Align::Center);
    let pill_row = gtk::Box::new(Orientation::Horizontal, ctx.px(10.0));
    let pill_dot = gtk::Box::new(Orientation::Horizontal, 0);
    pill_dot.add_css_class("dot");
    pill_dot.add_css_class("off");
    pill_dot.set_valign(Align::Center);
    pill_row.append(&pill_dot);
    let pill_text = gtk::Label::new(Some("Checking…"));
    pill_row.append(&pill_text);
    pill.set_child(Some(&pill_row));
    pill.connect_clicked(move |_| run(&privacy_app));
    right.append(&pill);
    let date = gtk::Label::new(None);
    date.add_css_class("date");
    date.set_margin_start(ctx.px(8.0));
    right.append(&date);
    let clock = gtk::Label::new(None);
    clock.add_css_class("clock");
    right.append(&clock);
    bar.set_end_widget(Some(&right));
    strip.set_child(Some(&bar));
    strip.present();
    panels::serve_notifications(&shell);
    shell.start();

    // ───── Live values ─────
    let live = Rc::new(Live { pill_dot, pill_text, date, clock, count, privacy_rows, checked, health, updates, signature });

    let tick_clock = {
        let live = live.clone();
        move || {
            if let Ok(now) = glib::DateTime::now_local() {
                live.date.set_text(&now.format("%a, %d %b %Y").map(|s| s.to_string()).unwrap_or_default());
                live.clock.set_text(&now.format("%H:%M").map(|s| s.to_string()).unwrap_or_default());
            }
        }
    };
    tick_clock();
    glib::timeout_add_seconds_local(5, move || {
        tick_clock();
        glib::ControlFlow::Continue
    });

    // Privacy settings are read on another thread every 30 seconds.
    let (send, receive) = mpsc::channel::<Privacy>();
    std::thread::spawn(move || loop {
        if send.send(read_privacy()).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(30));
    });

    let mut last_cpu = cpu_times();
    let icons = ctx.clone();
    let mut checked_at: Option<std::time::Instant> = None;
    let mut ticks = 0u32;
    let tick = move || {
        while let Ok(privacy) = receive.try_recv() {
            checked_at = Some(std::time::Instant::now());
            let good = privacy.iter().filter(|v| **v == Some(true)).count();
            let known = privacy.iter().filter(|v| v.is_some()).count();
            for ((mark, status), value) in live.privacy_rows.iter().zip(privacy) {
                let (text, class, icon) = match value {
                    Some(true) => ("Configured", "ok", "check-green"),
                    Some(false) => ("Needs attention", "warn", "status-warn"),
                    None => ("Unavailable", "dim", "status-unknown"),
                };
                status.set_text(text);
                for old in ["ok", "warn", "dim"] {
                    status.remove_css_class(old);
                }
                status.add_css_class(class);
                mark.set_from_file(Some(icons.cfg.file(&format!("{icon}.svg"))));
            }
            // The count and the pill never claim more than was actually read.
            if known == 0 {
                live.count.set_text("–");
                live.pill_text.set_text("Status unavailable");
                live.pill_dot.add_css_class("off");
            } else {
                live.count.set_text(&format!("{good}/6"));
                if good == 6 {
                    live.pill_text.set_text("All 6 Configured");
                    live.pill_dot.remove_css_class("off");
                } else {
                    live.pill_text.set_text(&format!("{good} of 6 Configured"));
                    live.pill_dot.add_css_class("off");
                }
            }
            live.updates.set_text(match privacy[5] {
                Some(true) => "Automatic updates on",
                Some(false) => "Automatic updates off",
                None => "Update status unavailable",
            });
            // what the policy file says, not a claim that this system was verified
            live.signature.set_text(match signature_policy() {
                Some(true) => "Signatures required",
                Some(false) => "Not required",
                None => "Unavailable",
            });
        }
        live.checked.set_text(&match checked_at.map(|at| at.elapsed().as_secs()) {
            None => "Not checked yet".to_string(),
            Some(s) if s < 60 => format!("Last checked: {s} seconds ago"),
            Some(s) => format!("Last checked: {} min ago", s / 60),
        });

        let cpu = cpu_times();
        let cpu_used = match (last_cpu, cpu) {
            (Some((t0, i0)), Some((t1, i1))) if t1 > t0 => Some(1.0 - (i1 - i0) as f64 / (t1 - t0) as f64),
            _ => None,
        };
        last_cpu = cpu;
        let temp = temperature();
        let disk = if ticks % 30 == 0 { disk_used() } else { None };
        ticks += 1;
        let readings = [
            (cpu_used, cpu_used.map(|v| format!("{:.0}%", v * 100.0))),
            (memory_used(), memory_used().map(|v| format!("{:.0}%", v * 100.0))),
            (disk, disk.map(|v| format!("{:.0}%", v * 100.0))),
            // drawn on a 20 to 100 °C scale
            (temp.map(|t| (t - 20.0) / 80.0), temp.map(|t| format!("{t:.0}°C"))),
        ];
        for (i, ((label, graph, history), (value, text))) in live.health.iter().zip(readings).enumerate() {
            let mut history = history.borrow_mut();
            // the disk is read less often; repeat its last value in between
            let value = value.or_else(|| if i == 2 { history.back().copied() } else { None });
            if let Some(text) = text {
                label.set_text(&text);
            }
            // amber from 90 % (85 °C for the temperature): worth a look, not an alarm
            if let Some(value) = value {
                let high = if i == 3 { value >= (85.0 - 20.0) / 80.0 } else { value >= 0.9 };
                if high { label.add_css_class("warn") } else { label.remove_css_class("warn") }
            }
            if let Some(value) = value {
                history.push_back(value);
                if history.len() > 24 {
                    history.pop_front();
                }
                graph.queue_draw();
            }
        }
    };
    let tick = Rc::new(RefCell::new(tick));
    (tick.borrow_mut())();
    glib::timeout_add_seconds_local(2, move || {
        (tick.borrow_mut())();
        glib::ControlFlow::Continue
    });
}

fn main() -> glib::ExitCode {
    if std::env::args().any(|arg| arg == "--print-config") {
        print!("{}", config::DEFAULTS);
        return glib::ExitCode::SUCCESS;
    }
    let app = gtk::Application::builder()
        .application_id("org.arkaos.Desk")
        // a theme change starts a second copy before the first one leaves
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_shutdown(|_| panels::shutdown());
    app.connect_activate(build);
    app.run()
}

#[cfg(test)]
mod tests {
    use super::{next_frame, scale_css, Grade};

    #[test]
    fn frames_go_back_and_forth() {
        let (mut at, mut forward) = (0, true);
        let mut seen = vec![0];
        for _ in 0..7 {
            (at, forward) = next_frame(at, forward, 4);
            seen.push(at);
        }
        assert_eq!(seen, [0, 1, 2, 3, 2, 1, 0, 1]);
    }

    #[test]
    fn the_wallpaper_grade() {
        assert_eq!(Grade::NONE.apply(10, 128, 250), (10, 128, 250));
        // no saturation leaves grey; less brightness darkens
        let grey = Grade { saturation: 0.0, ..Grade::NONE }.apply(255, 0, 0);
        assert!(grey.0 == grey.1 && grey.1 == grey.2);
        assert_eq!(Grade { brightness: 0.5, ..Grade::NONE }.apply(200, 100, 0), (100, 50, 0));
    }

    #[test]
    fn only_px_lengths_are_scaled() {
        assert_eq!(scale_css("a{min-width:84px;opacity:0.5}", 0.5), "a{min-width:42.00px;opacity:0.5}");
        assert_eq!(scale_css("b{color:#4FE08F;box-shadow:0 0 8px rgba(79,224,143,0.9)}", 2.0), "b{color:#4FE08F;box-shadow:0 0 16.00px rgba(79,224,143,0.9)}");
        assert_eq!(scale_css("c{border:1.5px solid}", 2.0), "c{border:3.00px solid}");
    }
}
