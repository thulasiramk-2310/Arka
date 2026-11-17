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
mod devices;
mod dock;
mod glass;
mod network;
mod panels;
mod x11;

use config::Config;

/// The next picture when going back and forth through `len` of them: 0, 1, .. last, .. 1, 0, 1.
fn next_frame(at: usize, forward: bool, len: usize) -> (usize, bool) {
    let forward = if at + 1 >= len { false } else if at == 0 { true } else { forward };
    (if forward { at + 1 } else { at - 1 }, forward)
}

/// Width of the screen the layout was drawn for.
const DESIGN_WIDTH: f64 = 1536.0;
/// The plain form of a green icon ("shield-green" is drawn as "shield-fill" until checked).
fn plain_icon(icon: &str) -> String {
    match icon.strip_suffix("-green") {
        Some(base) => format!("{base}-fill"),
        None => icon.to_string(),
    }
}

/// The layout is drawn for a 16:10 screen. On another shape it is scaled to fit and centred,
/// and the wallpaper still fills the whole screen.
const DESIGN_HEIGHT: f64 = 960.0;
/// Design x positions left of this belong to the left edge, right of `RIGHT_SIDE` to the right.
const LEFT_SIDE: f64 = 300.0;
const RIGHT_SIDE: f64 = 1100.0;

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

/// Whether this part, or what holds it, is folded away, shut or closed (its glass is not drawn).
fn put_away(widget: &gtk::Widget) -> bool {
    let mut at = Some(widget.clone());
    while let Some(part) = at {
        if ["shut", "folded", "gone", "cleared"].iter().any(|class| part.has_css_class(class)) {
            return true;
        }
        at = part.parent();
    }
    false
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

/// The lock screen in the desktop's look: the wallpaper, font and the theme's colours, for arka-lock
/// and for swaylock (used through `LOCK` when arka-lock is not installed and the user has no
/// swaylock config of their own or set ARKAWM_LOCKER).
fn write_lock_style(cfg: &config::Config) {
    let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") else { return };
    let hex = |key: &str, alpha: &str| format!("{}{alpha}", cfg.get(&format!("colors.{key}")).trim_start_matches('#'));
    let font = cfg.get("look.font").split(',').next().unwrap_or("sans-serif").trim().trim_matches('"').to_string();
    let chosen = Some(cfg.get("wallpaper.file")).filter(|file| !file.is_empty()).map(PathBuf::from);
    let picture = chosen.unwrap_or_else(|| cfg.file("wallpaper.jpg"));
    let mut style = format!(
        "color={}\nfont={font}\nindicator-radius=72\nindicator-thickness=7\nindicator-idle-visible\nshow-failed-attempts\n\
         ignore-empty-password\ninside-color={}\ninside-clear-color={}\ninside-ver-color={}\ninside-wrong-color={}\n\
         ring-color={}\nring-clear-color={}\nring-ver-color={}\nring-wrong-color={}\nkey-hl-color={}\nbs-hl-color={}\n\
         line-color=00000000\nline-clear-color=00000000\nline-ver-color=00000000\nline-wrong-color=00000000\n\
         separator-color=00000000\ntext-color={}\ntext-clear-color={}\ntext-ver-color={}\ntext-wrong-color={}\n",
        hex("background", "ff"),
        hex("panel", "e0"), hex("panel", "e0"), hex("panel", "e0"), hex("panel", "e0"),
        hex("border", "66"), hex("text_dim", "ff"), hex("accent", "ff"), hex("warn", "ff"),
        hex("accent", "ff"), hex("warn", "ff"),
        hex("text", "ff"), hex("text", "ff"), hex("text", "ff"), hex("warn", "ff"),
    );
    if config::is_picture(&picture) && picture.exists() {
        style.push_str(&format!("image={}\nscaling=fill\n", picture.display()));
    }
    let _ = std::fs::write(format!("{dir}/arka-desk-swaylock.conf"), style);
    // arka-lock, the ArkaWM lock screen, takes the same picture, font and colours.
    let icons = cfg.file("power-bold.svg");
    let icons = icons.parent().map(|dir| dir.display().to_string()).unwrap_or_default();
    let mut look = format!("font = {font}\nwallpaper = {}\nicons = {icons}\n", picture.display());
    for key in ["background", "panel", "border", "text", "text_dim", "accent", "warn"] {
        look.push_str(&format!("{key} = {}\n", cfg.get(&format!("colors.{key}"))));
    }
    let _ = std::fs::write(format!("{dir}/arka-desk-lock.conf"), look);
}

/// How the Lock button starts the locker: with the desktop's lock style when nothing else was chosen.
const LOCK: &str = r#"style="$XDG_RUNTIME_DIR/arka-desk-swaylock.conf"; if [ -n "$ARKAWM_LOCKER" ]; then exec $ARKAWM_LOCKER; elif command -v arka-lock >/dev/null; then exec arka-lock; elif [ -f "$style" ] && [ ! -e "${XDG_CONFIG_HOME:-$HOME/.config}/swaylock/config" ] && [ ! -e "$HOME/.swaylock/config" ]; then exec swaylock -C "$style"; else exec swaylock; fi"#;

/// The theme's glyph for an app of this kind, so a picked app looks like the rest of the ring.
fn glyph_for(categories: &str) -> Option<&'static str> {
    let has = |kind: &str| categories.split(';').any(|c| c == kind);
    [
        ("WebBrowser", "globe-bold"),
        ("TerminalEmulator", "terminal-bold"),
        ("FileManager", "folder-fill"),
        ("PackageManager", "storefront-bold"),
        ("Security", "shield-fill"),
        ("Monitor", "cube-fill"),
        ("Settings", "gear-fill"),
        ("IDE", "code-bold"),
        ("TextEditor", "code-bold"),
        ("Development", "code-bold"),
        ("Photography", "image-bold"),
        ("Graphics", "image-bold"),
        ("Music", "headphones-bold"),
        ("Audio", "headphones-bold"),
    ]
    .into_iter()
    .find(|(kind, _)| has(kind))
    .map(|(_, glyph)| glyph)
}

/// The ring line for an installed app: its name, a glyph (or its own icon), and how to start it.
fn ring_line(app: &gtk::gio::DesktopAppInfo) -> Option<String> {
    let file = app.filename()?;
    let name = app.display_name().replace(['|', ';', '\n'], " ");
    let icon = match glyph_for(&app.categories().map(|c| c.to_string()).unwrap_or_default()) {
        Some(glyph) => glyph.to_string(),
        None => match app.icon().and_then(|icon| icon.downcast::<gtk::gio::ThemedIcon>().ok()) {
            Some(themed) => format!("app:{}", themed.names().first().map(|n| n.to_string()).unwrap_or_default()),
            None => "squares-four-fill".to_string(),
        },
    };
    let path = file.to_string_lossy().replace('\'', "'\\''");
    Some(format!("{name} | {icon} | gio launch '{path}'"))
}

/// Where the cards on the right sit, in design pixels: gap to the right edge, gap to the top, width.
const CARDS: (f64, f64, f64) = (22.0, 77.0, 300.0);
const CARDS_NARROWEST: f64 = 260.0;
const CARDS_WIDEST: f64 = 560.0;

/// The cards' place as the user left it, from ~/.config/arkawm/cards.
fn load_cards() -> Option<(f64, f64, f64)> {
    let text = std::fs::read_to_string(panels::settings_dir()?.join("cards")).ok()?;
    let mut values = std::collections::HashMap::new();
    config::parse(&text, &mut values);
    let number = |key: &str| values.get(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
    Some((number("cards.right")?, number("cards.top")?, number("cards.width")?.clamp(CARDS_NARROWEST, CARDS_WIDEST)))
}

/// Remember the cards' place (`None` forgets it, so they go back to where they started).
fn save_cards(layout: Option<(f64, f64, f64)>) {
    let Some(dir) = panels::settings_dir() else { return };
    let file = dir.join("cards");
    let result = match layout {
        Some((right, top, wide)) => std::fs::create_dir_all(&dir).and_then(|_| {
            std::fs::write(&file, format!(
                "# Where the cards on the right sit, in design pixels. Delete this file to put them back.\n[cards]\nright = {right:.0}\ntop = {top:.0}\nwidth = {wide:.0}\n"
            ))
        }),
        None => std::fs::remove_file(&file).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
    };
    if let Err(err) = result {
        eprintln!("arka-desk: could not save where the cards are: {err}");
    }
}

/// Save what this place on the ring holds (`None` puts back the original) and start again with it.
fn save_place(place: usize, line: Option<String>) {
    let Some(dir) = panels::settings_dir() else { return };
    let file = dir.join("ring");
    let mut places = std::collections::HashMap::new();
    config::parse(&std::fs::read_to_string(&file).unwrap_or_default(), &mut places);
    let key = format!("apps.{place}");
    match line {
        Some(line) => places.insert(key, line),
        None => places.remove(&key),
    };
    let changed: Vec<String> =
        (1..=8).filter_map(|n| places.get(&format!("apps.{n}")).map(|line| format!("{n} = {line}\n"))).collect();
    let saved = if changed.is_empty() {
        // nothing left changed: the ring is the original again
        std::fs::remove_file(&file).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })
    } else {
        let text = format!("# The ring's places as changed from the desktop. Delete a line to put the original back.\n[apps]\n{}", changed.concat());
        std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, text))
    };
    if saved.is_ok() {
        panels::start_again();
    }
}

/// A right-click on a ring circle: choose another installed app for that place. Drawn inside the
/// desktop beside the circle (a pop-up would not get the keyboard for the search box).
fn offer_apps(node: &gtk::Widget, overlay: &gtk::Overlay, place: usize, now: &str) {
    use gtk::gio;
    // one chooser at a time
    let mut child = overlay.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class("app-chooser") {
            overlay.remove_overlay(&widget);
        }
    }
    let column = gtk::Box::new(Orientation::Vertical, 8);
    column.add_css_class("chooser");
    column.add_css_class("app-chooser");
    column.set_halign(Align::Start);
    column.set_valign(Align::Start);
    let close: Rc<dyn Fn()> = {
        let (overlay, column) = (overlay.clone(), column.clone());
        Rc::new(move || {
            if column.parent().is_some() {
                overlay.remove_overlay(&column);
            }
        })
    };
    let head = gtk::Box::new(Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(&format!("Replace {now}")));
    title.add_css_class("title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    head.append(&title);
    let shut = gtk::Button::with_label("Cancel");
    shut.add_css_class("link");
    shut.connect_clicked({
        let close = close.clone();
        move |_| close()
    });
    head.append(&shut);
    column.append(&head);
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let close = close.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    column.add_controller(keys);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Find an app"));
    column.append(&search);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    let mut apps: Vec<gio::DesktopAppInfo> = gio::AppInfo::all()
        .into_iter()
        .filter(|app| app.should_show())
        .filter_map(|app| app.downcast::<gio::DesktopAppInfo>().ok())
        .collect();
    apps.sort_by_key(|app| app.display_name().to_lowercase());
    apps.dedup_by_key(|app| app.display_name().to_lowercase());
    // the apps used from search come first, as in search
    let counts = panels::launch_counts();
    apps.sort_by(|a, b| {
        let (ia, ib) = (a.id().map(|i| i.to_string()).unwrap_or_default(), b.id().map(|i| i.to_string()).unwrap_or_default());
        panels::by_use(&counts, (&ia, &a.display_name()), (&ib, &b.display_name()))
    });
    for app in apps {
        let Some(line) = ring_line(&app) else { continue };
        let row = gtk::Box::new(Orientation::Horizontal, 10);
        if let Some(icon) = app.icon() {
            let image = gtk::Image::from_gicon(&icon);
            image.set_pixel_size(24);
            row.append(&image);
        }
        let label = gtk::Label::new(Some(&app.display_name()));
        label.set_xalign(0.0);
        row.append(&label);
        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("item");
        button.set_widget_name(&app.display_name().to_lowercase());
        let close = close.clone();
        button.connect_clicked(move |_| {
            close();
            save_place(place, Some(line.clone()));
        });
        list.append(&button);
    }
    list.set_filter_func({
        let search = search.clone();
        move |row| {
            let wanted = search.text().to_lowercase();
            wanted.is_empty() || row.child().is_some_and(|child| child.widget_name().contains(wanted.trim()))
        }
    });
    search.connect_search_changed({
        let list = list.clone();
        move |_| list.invalidate_filter()
    });
    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(320)
        .min_content_width(280)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    column.append(&scroll);
    let reset = gtk::Button::with_label("Put back the original");
    reset.add_css_class("item");
    reset.connect_clicked({
        let close = close.clone();
        move |_| {
            close();
            save_place(place, None);
        }
    });
    column.append(&reset);
    // beside the circle, kept on the screen
    let at = node.compute_point(overlay, &gtk::graphene::Point::new(node.width() as f32 + 12.0, 0.0)).unwrap_or(gtk::graphene::Point::new(0.0, 0.0));
    let (room_x, room_y) = (overlay.width() as f32 - 340.0, overlay.height() as f32 - 480.0);
    column.set_margin_start(at.x().min(room_x).max(16.0) as i32);
    column.set_margin_top(at.y().min(room_y).max(16.0) as i32);
    overlay.add_overlay(&column);
    search.grab_focus();
}

thread_local! {
    /// Apps on the ring that were clicked and have no window yet; their circle pulses meanwhile.
    static STARTING: RefCell<Vec<gtk::Widget>> = const { RefCell::new(Vec::new()) };
}

/// A window has appeared: the apps that were starting have started.
fn started() {
    STARTING.with(|list| list.borrow_mut().drain(..).for_each(|widget| widget.remove_css_class("starting")));
}

/// Start the first of these commands that works.
fn run(commands: &[String]) {
    start(commands, None);
}

/// Start the first of these commands that works, with `button` pulsing until a window appears.
/// A program that is there but fails at once (a launcher left behind when its app was removed,
/// say) counts as missing, and the next one is tried.
fn start(commands: &[String], button: Option<gtk::Widget>) {
    let ready: VecDeque<String> =
        commands.iter().filter(|command| installed(command.split(' ').next().unwrap_or_default())).cloned().collect();
    if let Some(button) = &button {
        if !ready.is_empty() {
            button.add_css_class("starting");
            STARTING.with(|list| list.borrow_mut().push(button.clone()));
            // some apps open no window, or one the strip does not list: stop pulsing after a while
            let button = button.clone();
            glib::timeout_add_seconds_local_once(12, move || button.remove_css_class("starting"));
        }
    }
    try_next(ready, button, commands.to_vec());
}

fn try_next(mut ready: VecDeque<String>, button: Option<gtk::Widget>, mut all: Vec<String>) {
    let Some(command) = ready.pop_front() else {
        if let Some(button) = &button {
            button.remove_css_class("starting");
        }
        let programs: Vec<&str> = all.iter().filter_map(|command| command.split(' ').next()).collect();
        eprintln!("arka-desk: none of {programs:?} could be started");
        let _ = Command::new("notify-send")
            .args(["-a", "ArkaOS", "Nothing opened", &format!("None of these could be started: {}.", programs.join(", "))])
            .spawn()
            .map(|mut child| std::thread::spawn(move || child.wait()));
        return;
    };
    let program = command.split(' ').next().unwrap_or_default().to_string();
    let line = match FUZZEL_STYLE.get() {
        Some(style) if program == "fuzzel" && !command.contains("--config") => format!("{command} --config '{style}'"),
        _ => command.clone(),
    };
    let Ok(child) = x11::shell(&line).spawn() else {
        return try_next(ready, button, all);
    };
    let since = std::time::Instant::now();
    let mut child = Some(child);
    glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
        let Some(running) = child.as_mut() else { return glib::ControlFlow::Break };
        match running.try_wait() {
            Ok(Some(status)) if !status.success() && since.elapsed().as_secs() < 3 => {
                eprintln!("arka-desk: {program} failed at once ({status}); trying the next one");
                try_next(std::mem::take(&mut ready), button.clone(), std::mem::take(&mut all));
                glib::ControlFlow::Break
            }
            Ok(Some(_)) | Err(_) => glib::ControlFlow::Break,
            Ok(None) if since.elapsed().as_secs() < 3 => glib::ControlFlow::Continue,
            Ok(None) => {
                // it is running: leave a thread to collect it when it ends
                if let Some(mut running) = child.take() {
                    std::thread::spawn(move || running.wait());
                }
                glib::ControlFlow::Break
            }
        }
    });
}

/// Shares of `look.blur` used behind glass: round buttons, the menu and small cards, the
/// information cards. With the default of 8 that is 4, 6 and 8 design pixels.
const GLASS_LEVELS: [f64; 3] = [0.5, 0.75, 1.0];
const ROUND: f64 = 0.5;
const SMALL: f64 = 0.75;
const CARD: f64 = 1.0;

struct Ctx {
    k: f64,
    /// Where the scaled layout starts on the screen, in pixels: the margins left when the
    /// screen is wider or taller than 16:10.
    ox: f64,
    oy: f64,
    cfg: Config,
    /// Glass surfaces on the desktop and their corner radius in design pixels (negative: round).
    /// A softened copy of the wallpaper is painted behind each, so they read as frosted glass.
    /// The third number is how much of the glass blur this surface gets (see `GLASS_LEVELS`).
    frost: RefCell<Vec<(gtk::Widget, f64, f64)>>,
    /// The Privacy entries (the ring circle and the menu pill), with their picture and its name.
    /// They turn green only once the privacy service has answered and all six settings were read
    /// as configured; until then they look like every other entry.
    verified: RefCell<Vec<(gtk::Widget, gtk::Image, String)>>,
    /// Softened copies of the wallpaper, one per blur level, small; stretched when painted.
    /// Only these are blurred. The wallpaper itself is shown as it is.
    soft: RefCell<Vec<(f64, gtk::cairo::ImageSurface)>>,
    /// The same copies as textures, for the glass layer (drawn by the GPU).
    soft_textures: RefCell<Vec<(f64, gtk::gdk::Texture)>>,
    /// Windows are up over the desktop: the living wallpaper holds still, unseen.
    covered: Cell<bool>,
}

impl Ctx {
    fn frosted(&self, widget: &impl IsA<gtk::Widget>, radius: f64, level: f64) {
        self.frost.borrow_mut().push((widget.clone().upcast(), radius, level));
    }
    fn soft(&self, level: f64) -> Option<gtk::cairo::ImageSurface> {
        self.soft.borrow().iter().find(|(l, _)| *l == level).map(|(_, surface)| surface.clone())
    }
    fn soft_texture(&self, level: f64) -> Option<gtk::gdk::Texture> {
        self.soft_textures.borrow().iter().find(|(l, _)| *l == level).map(|(_, texture)| texture.clone())
    }
    /// Where a design x lands on the screen. When the screen is wider than 16:10 the layout is
    /// not just centred: what sits on the left keeps to the left edge and what sits on the right
    /// keeps to the right edge, so nothing floats in from the sides (reported on a 16:9 laptop).
    fn screen_x(&self, x: f64) -> f64 {
        let shift = if x < LEFT_SIDE { 0.0 } else if x > RIGHT_SIDE { 2.0 * self.ox } else { self.ox };
        x * self.k + shift
    }
    fn px(&self, v: f64) -> i32 {
        (v * self.k).round() as i32
    }
    fn icon(&self, name: &str, size: f64) -> gtk::Image {
        // "app:<icon>": an app picked for the ring that has no glyph of the theme's own.
        let image = match name.strip_prefix("app:") {
            Some(theme_icon) => gtk::Image::from_icon_name(theme_icon),
            None => gtk::Image::from_file(self.cfg.file(&format!("{name}.svg"))),
        };
        image.set_pixel_size(self.px(size));
        image
    }
    /// Green for the Privacy entries only when the privacy settings were read and all are on.
    fn show_verified(&self, verified: bool) {
        for (widget, image, icon) in self.verified.borrow().iter() {
            if verified {
                widget.add_css_class("green");
            } else {
                widget.remove_css_class("green");
            }
            let shown = if verified { icon.clone() } else { plain_icon(icon) };
            if !shown.starts_with("app:") {
                image.set_from_file(Some(self.cfg.file(&format!("{shown}.svg"))));
            }
        }
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
        widget.set_margin_start(self.screen_x(x).round() as i32);
        widget.set_margin_top(self.px(y) + self.oy.round() as i32);
        overlay.add_overlay(widget);
    }
    /// Place a widget centred across the screen at a design height.
    fn put_center(&self, overlay: &gtk::Overlay, widget: &impl IsA<gtk::Widget>, y: f64) {
        widget.set_halign(Align::Center);
        widget.set_valign(Align::Start);
        widget.set_margin_top(self.px(y) + self.oy.round() as i32);
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
        disc.set_halign(Align::Center);
        self.frosted(&disc, -1.0, ROUND);
        let image = self.icon(&plain_icon(icon), 40.0);
        if green {
            self.verified.borrow_mut().push((disc.clone().upcast(), image.clone(), icon.to_string()));
        }
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
        button.connect_clicked(move |button| start(&commands, Some(button.clone().upcast())));
        button
    }
    fn pill(&self, icon: &str, name: &str, width: f64, green: bool) -> gtk::Button {
        let row = gtk::Box::new(Orientation::Horizontal, self.px(22.0));
        let image = self.icon(&plain_icon(icon), 22.0);
        row.append(&image);
        row.append(&gtk::Label::new(Some(name)));
        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("pill");
        if green {
            self.verified.borrow_mut().push((button.clone().upcast(), image.clone(), icon.to_string()));
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
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 0.45);
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
        cr.set_line_width(2.0);
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

/// The privacy daemon holds its name on the system bus. On a computer that is not running ArkaOS
/// (or where the daemon is stopped) every row would only say "Unavailable".
fn arkad_running() -> bool {
    Command::new("timeout")
        .args(["3", "busctl", "--system", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s", "org.arka.arkad"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "b true")
}

/// An ArkaOS (bootc) system. Elsewhere the update timer and the image signature policy do not apply.
/// Whether the privacy service is installed here (it may still be stopped).
fn arkad_installed() -> bool {
    ["/usr/lib/systemd/system/arkad.service", "/etc/systemd/system/arkad.service"].iter().any(|unit| Path::new(unit).exists())
}

fn on_image() -> bool {
    Path::new("/run/ostree-booted").exists()
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
    // ArkaOS writes its own version to arkaos-release; os-release's VERSION_ID is the base's
    let own = std::fs::read_to_string("/usr/lib/arkaos-release").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    own.or_else(|| field("IMAGE_VERSION="))
        .or_else(|| field("VERSION="))
        .or_else(|| field("VERSION_ID="))
        .or_else(|| field("BUILD_ID=").map(|id| if id == "rolling" { "Rolling release".into() } else { id }))
        .unwrap_or_else(|| "Unknown".into())
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

/// A small, soft copy of a wallpaper for painting behind glass, for cairo and as a texture.
/// `blur` is in screen pixels.
/// Running on battery: a battery is there and no charger is plugged in.
fn on_battery() -> bool {
    let mut battery = false;
    for entry in std::fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten() {
        let read = |name: &str| std::fs::read_to_string(entry.path().join(name)).unwrap_or_default();
        match read("type").trim() {
            "Mains" | "USB" if read("online").trim() == "1" => return false,
            "Battery" => battery = true,
            _ => {}
        }
    }
    battery
}

fn softened(pixbuf: &gtk::gdk_pixbuf::Pixbuf, blur: f64) -> Option<(gtk::cairo::ImageSurface, gtk::gdk::Texture)> {
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
    #[allow(deprecated)]
    let texture = gtk::gdk::Texture::for_pixbuf(&small);
    Some((surface, texture))
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
    privacy_lines: Vec<gtk::Box>,
    privacy_note: gtk::Label,
    signature_row: gtk::Box,
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
    dock::follow(cfg.get("dock.command"));
    let ring_rgb = {
        // the ring is the accent, lifted towards white so it reads as light
        let (r, g, b) = cfg.color("accent");
        (0.5 + r * 0.5, 0.5 + g * 0.5, 0.5 + b * 0.5)
    };
    let (accent_rgb, ok_rgb) = (cfg.color("accent"), cfg.color("ok"));
    let display = gtk::gdk::Display::default().expect("no display");
    let (width, height) = display
        .monitors()
        .item(0)
        .and_then(|m| m.downcast::<gtk::gdk::Monitor>().ok())
        .map(|m| (m.geometry().width(), m.geometry().height()))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((1536, 960));
    // the scale that fits the whole layout on this screen, whatever its shape
    let k = (width as f64 / DESIGN_WIDTH).min(height as f64 / DESIGN_HEIGHT);
    let (ox, oy) = ((width as f64 - DESIGN_WIDTH * k) / 2.0, (height as f64 - DESIGN_HEIGHT * k) / 2.0);
    let ctx = Rc::new(Ctx { k, ox, oy, cfg, frost: RefCell::new(Vec::new()), soft: RefCell::new(Vec::new()), soft_textures: RefCell::new(Vec::new()), verified: RefCell::new(Vec::new()), covered: Cell::new(false) });
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
    // Moving with the keyboard shows where the keyboard is (GTK does not always switch the ring on
    // for a layer surface that got the keyboard without a click); a click hides the ring again.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let desk = desk.clone();
        move |_, key, _, _| {
            use gtk::gdk::Key;
            if matches!(key, Key::Tab | Key::ISO_Left_Tab | Key::Left | Key::Right | Key::Up | Key::Down) {
                desk.set_focus_visible(true);
            }
            glib::Propagation::Proceed
        }
    });
    desk.add_controller(keys);
    let clicks = gtk::GestureClick::new();
    clicks.set_propagation_phase(gtk::PropagationPhase::Capture);
    clicks.connect_pressed({
        let desk = desk.clone();
        move |_, _, _, _| desk.set_focus_visible(false)
    });
    desk.add_controller(clicks);
    desk.set_default_size(width, height);
    let overlay = gtk::Overlay::new();
    // The glass: one layer above the wallpaper paints its softened copy where each glass
    // surface sits. (Only on the desktop; sheets and the strip sit over windows, and blurring
    // those needs the compositor.)
    let veil = cfg.number("look.frost", 0.10).clamp(0.0, 0.6) as f32;
    // Frosted glass is a little lighter than what is behind it: a pale veil over the softened
    // picture, only inside the glass (look.frost; 0 leaves the softened picture as it is).
    let frost = glass::Glass::new(Box::new({
        let ctx = ctx.clone();
        move |area, snapshot| {
            let (w, h) = (area.width() as f32, area.height() as f32);
            for (widget, radius, level) in ctx.frost.borrow().iter() {
                if !widget.is_mapped() || put_away(widget) {
                    continue;
                }
                let Some(texture) = ctx.soft_texture(*level) else { continue };
                let Some(bounds) = widget.compute_bounds(area) else { continue };
                // the softened picture covers the screen as the wallpaper does; only the part
                // inside this surface's rounded outline is drawn
                let scale = (w / texture.width() as f32).max(h / texture.height() as f32);
                let (tw, th) = (texture.width() as f32 * scale, texture.height() as f32 * scale);
                let cover = gtk::graphene::Rect::new((w - tw) / 2.0, (h - th) / 2.0, tw, th);
                let short = bounds.width().min(bounds.height());
                let r = if *radius < 0.0 { short / 2.0 } else { (*radius as f32 * ctx.k as f32).min(short / 2.0) };
                snapshot.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(bounds, r));
                snapshot.append_scaled_texture(&texture, gtk::gsk::ScalingFilter::Linear, &cover);
                snapshot.append_color(&gtk::gdk::RGBA::new(0.86, 0.90, 0.97, veil), &bounds);
                snapshot.pop();
            }
        }
    }));
    frost.set_can_target(false);
    frost.add_css_class("frost");
    let glass_blur = cfg.number("look.blur", 8.0).clamp(0.0, 80.0) * k;
    let picture = {
        let (ctx, frost) = (ctx.clone(), frost.clone());
        Rc::new(move |path: &Path| {
            let picture = match graded(path, width, &ctx.cfg, ctx.k) {
                Some(pixbuf) => {
                    if glass_blur > 0.0 {
                        let copies: Vec<(f64, gtk::cairo::ImageSurface, gtk::gdk::Texture)> = GLASS_LEVELS
                            .iter()
                            .filter_map(|level| softened(&pixbuf, glass_blur * level).map(|(surface, texture)| (*level, surface, texture)))
                            .collect();
                        *ctx.soft.borrow_mut() = copies.iter().map(|(level, surface, _)| (*level, surface.clone())).collect();
                        *ctx.soft_textures.borrow_mut() = copies.into_iter().map(|(level, _, texture)| (level, texture)).collect();
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
        stack.set_transition_duration(if motion { cfg.number("wallpaper.fade", 4000.0).clamp(0.0, 60000.0) as u32 } else { 0 });
        let mut pictures: Vec<Option<gtk::Picture>> = frames.iter().map(|_| None).collect();
        let first = picture(&frames[0]);
        stack.add_child(&first);
        pictures[0] = Some(first);
        overlay.set_child(Some(&stack));
        let (mut at, mut forward) = (0usize, true);
        let (stack, picture, ctx) = (stack.clone(), picture.clone(), ctx.clone());
        glib::timeout_add_seconds_local(cfg.number("wallpaper.every", 60.0).clamp(5.0, 86400.0) as u32, move || {
            // A fade redraws the whole screen for seconds and each picture is decoded again: not
            // while windows are open, and not on battery.
            if ctx.covered.get() || on_battery() {
                return glib::ControlFlow::Continue;
            }
            let before = at;
            (at, forward) = next_frame(at, forward, frames.len());
            // keep only the picture fading out; the others would hold a full screen each
            for (i, slot) in pictures.iter_mut().enumerate() {
                if i != before {
                    if let Some(old) = slot.take() {
                        stack.remove(&old);
                    }
                }
            }
            // loading the picture again also refreshes the glass behind the cards
            let shown = picture(&frames[at]);
            stack.add_child(&shown);
            stack.set_visible_child(&shown);
            pictures[at] = Some(shown);
            glib::ControlFlow::Continue
        });
    } else {
        overlay.set_child(Some(&picture(&own("wallpaper.file", "wallpaper.jpg"))));
    }

    // Breathing light: the glowing paths in the rocks grow softly brighter and back. Only over
    // the theme's own picture (the glow is drawn for it), and off unless asked for: it repaints
    // the screen a few times a second, which costs a little power the whole time.
    let glow_file = cfg.file("glow.png");
    if cfg.on("wallpaper.breathe") && motion && cfg.get("wallpaper.file").is_empty() && glow_file.is_file() {
        let glow = gtk::Picture::for_filename(&glow_file);
        glow.set_can_shrink(true);
        glow.set_content_fit(gtk::ContentFit::Cover);
        glow.set_can_target(false);
        glow.set_opacity(0.0);
        overlay.add_overlay(&glow);
        let period = cfg.number("wallpaper.breathe_every", 7.0).clamp(2.0, 120.0);
        let peak = cfg.number("wallpaper.breathe_strength", 1.0).clamp(0.0, 1.0);
        let started = std::time::Instant::now();
        glib::timeout_add_local(std::time::Duration::from_millis(66), move || {
            let phase = started.elapsed().as_secs_f64() / period * std::f64::consts::TAU;
            glow.set_opacity(peak * (0.5 - 0.5 * phase.cos()));
            glib::ControlFlow::Continue
        });
    }
    overlay.add_overlay(&frost);
    // Entrance and menu animations move the glass: paint it every frame until they have settled
    // (900 ms covers the longest, the ring's apps), so it moves with its cards.
    let refrost = {
        let frost = frost.clone();
        let until = Rc::new(Cell::new(0i64));
        let running = Rc::new(Cell::new(false));
        Rc::new(move || {
            frost.queue_draw();
            let Some(clock) = frost.frame_clock() else {
                for ms in [120, 360, 900] {
                    let frost = frost.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || frost.queue_draw());
                }
                return;
            };
            until.set(clock.frame_time() + 900_000);
            if running.replace(true) {
                return;
            }
            let (until, running) = (until.clone(), running.clone());
            frost.add_tick_callback(move |frost, clock| {
                frost.queue_draw();
                if clock.frame_time() >= until.get() {
                    running.set(false);
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
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
    ring.add_css_class("ringlights");
    ring.set_content_width(ctx.px(560.0));
    ring.set_content_height(ctx.px(560.0));
    ring.set_can_target(false);
    // How much of the ring is drawn, clockwise from the top: 1 whole, 0 none. Folding the apps
    // away winds it back; bringing them out draws it round again (see the mark below).
    let sweep = Rc::new(Cell::new(1.0f64));
    ring.set_draw_func({
        let sweep = sweep.clone();
        move |_, cr, _, _| {
            use std::f64::consts::{FRAC_PI_2, TAU};
            let share = sweep.get();
            if share <= 0.0 {
                return;
            }
            // how far round from the top this angle is, 0..1
            let along = |angle: f64| (angle + FRAC_PI_2).rem_euclid(TAU) / TAU;
            let (start, end) = (-FRAC_PI_2, -FRAC_PI_2 + TAU * share);
            cr.scale(k, k);
            let (r, g, b) = ring_rgb;
            for (width, alpha) in [(9.0, 0.06), (5.0, 0.11), (1.4, 0.62)] {
                cr.set_source_rgba(r, g, b, alpha);
                cr.set_line_width(width);
                cr.new_sub_path();
                cr.arc(280.0, 280.0, 224.0, start, end);
                let _ = cr.stroke();
            }
            cr.set_source_rgba(r, g, b, 0.38);
            cr.set_line_width(1.0);
            cr.new_sub_path();
            cr.arc(280.0, 280.0, 162.0, start, end);
            let _ = cr.stroke();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
            for i in 0..8 {
                let angle = TAU * (i as f64 + 0.5) / 8.0;
                if along(angle) <= share {
                    cr.arc(280.0 + 224.0 * angle.cos(), 280.0 + 224.0 * angle.sin(), 2.6, 0.0, TAU);
                    let _ = cr.fill();
                }
            }
            for i in 0..4 {
                let angle = TAU * i as f64 / 4.0;
                if along(angle) <= share {
                    cr.arc(280.0 + 162.0 * angle.cos(), 280.0 + 162.0 * angle.sin(), 2.0, 0.0, TAU);
                    let _ = cr.fill();
                }
            }
        }
    });
    if show_ring {
        ctx.put(&overlay, &ring, 488.0, 96.0);
    }
    // Draw the ring round to `to` (1 whole, 0 none) over `ms`, eased in and out; the apps' delays
    // in style.css are timed to this curve.
    let wind: Rc<dyn Fn(f64, f64)> = {
        let (sweep, ring) = (sweep.clone(), ring.clone());
        let running: Rc<RefCell<Option<gtk::TickCallbackId>>> = Rc::new(RefCell::new(None));
        Rc::new(move |to: f64, ms: f64| {
            if let Some(old) = running.borrow_mut().take() {
                old.remove();
            }
            let ms = if motion { ms } else { 0.0 };
            let from = sweep.get();
            // one step per screen refresh, timed by the frame clock, so the line moves evenly
            let started: Cell<Option<i64>> = Cell::new(None);
            let (sweep, slot) = (sweep.clone(), running.clone());
            let id = ring.add_tick_callback(move |ring, clock| {
                let now = clock.frame_time();
                let begin = *started.get().get_or_insert(now);
                started.set(Some(begin));
                let t = if ms <= 0.0 { 1.0 } else { ((now - begin) as f64 / 1000.0 / ms).min(1.0) };
                sweep.set(from + (to - from) * (0.5 - 0.5 * (std::f64::consts::PI * t).cos()));
                ring.queue_draw();
                if t >= 1.0 {
                    slot.borrow_mut().take();
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
            *running.borrow_mut() = Some(id);
        })
    };
    // on start the ring draws itself round as the apps settle onto it
    if motion {
        sweep.set(0.0);
        let wind = wind.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || wind(1.0, 600.0));
    }

    let centre = gtk::Box::new(Orientation::Vertical, 0);
    centre.set_size_request(ctx.px(400.0), -1);
    centre.add_css_class("centre");
    // The mark is a control: it folds the apps into it and brings them back (see below).
    let mark = gtk::Button::new();
    mark.add_css_class("mark");
    mark.set_child(Some(&ctx.logo(196.0)));
    mark.set_halign(Align::Center);
    mark.set_tooltip_text(Some("Hide the apps"));
    mark.update_property(&[gtk::accessible::Property::Label("Hide the apps")]);
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
    let mut nodes: Vec<gtk::Button> = Vec::new();
    // the tip under the ring; the menu's "Tips" shows it again
    let tip_slot: Rc<RefCell<Option<gtk::Box>>> = Rc::new(RefCell::new(None));
    if show_ring {
        for (i, ((name, icon, commands, highlight), (x, y))) in cfg.apps().into_iter().zip(PLACES).enumerate() {
            let node = ctx.node(&icon, &name, highlight, commands);
            node.add_css_class(&format!("n{}", i + 1));
            let pick = gtk::GestureClick::new();
            pick.set_button(3);
            pick.connect_pressed({
                let (overlay, name) = (overlay.clone(), name.clone());
                move |gesture, _, _, _| {
                    if let Some(node) = gesture.widget() {
                        offer_apps(&node, &overlay, i + 1, &name);
                    }
                }
            });
            node.add_controller(pick);
            node.set_tooltip_text(Some(&format!("{name}\nRight-click to put another app here")));
            node.update_property(&[gtk::accessible::Property::Label(&name)]);
            ctx.put(&overlay, &node, x, y);
            nodes.push(node);
        }
        // The first time only: what the ring, the mark and the round button do.
        let seen = panels::settings_dir().map(|dir| dir.join("tips-seen"));
        {
            let tip = gtk::Box::new(Orientation::Horizontal, ctx.px(16.0));
            tip.add_css_class("chooser");
            tip.add_css_class("tip");
            let words = gtk::Label::new(Some(
                "Click a circle to open its app, or right-click it to put another app there.\n\
                 Click the logo in the middle to fold the ring away. The round button on the left opens the menu.\n\
                 Double-click the wallpaper to put the apps and cards away, and again to bring them back.",
            ));
            words.set_xalign(0.0);
            tip.append(&words);
            let ok = gtk::Button::with_label("Got it");
            ok.add_css_class("link");
            ok.set_valign(Align::Center);
            ok.connect_clicked({
                let (tip, seen) = (tip.clone(), seen.clone());
                move |_| {
                    if let Some(file) = &seen {
                        let _ = file.parent().map(std::fs::create_dir_all);
                        let _ = std::fs::write(file, "");
                    }
                    tip.set_visible(false);
                }
            });
            tip.append(&ok);
            tip.set_visible(seen.as_ref().is_some_and(|file| !file.exists()));
            ctx.put_center(&overlay, &tip, 760.0);
            *tip_slot.borrow_mut() = Some(tip);
        }
        if let Some(text) = caption("platform") {
            ctx.put_center(&overlay, &ctx.caps(text, &[], true, 40.0), 724.0);
        }
    }
    write_fuzzel_style(cfg);
    write_lock_style(cfg);
    let privacy_app = Rc::new(cfg.commands("privacy"));

    // The menu on the left. Only the round button shows; a click on it opens the menu.
    let mut menu_parts: Vec<gtk::Widget> = Vec::new();
    // The menu's body and the lobe around the round button are one shape with one outline:
    // the button grows into the menu. Drawn here because a style sheet cannot join two shapes.
    // Opening, the shape grows out of the round button with a little overshoot and the entries
    // fan out of it; shutting, it all draws back in, quicker.
    // The menu's panel is drawn in its own box, not across the whole screen: it is redrawn every
    // frame while it unfolds, and a full-screen software redraw made that take seconds.
    const BODY_BOX: (f64, f64, f64, f64) = (36.0, 400.0, 414.0, 326.0);
    let body = gtk::DrawingArea::new();
    body.set_can_target(false);
    body.set_content_width(ctx.px(BODY_BOX.2));
    body.set_content_height(ctx.px(BODY_BOX.3));
    let body_share = Rc::new(Cell::new(0.0f64));
    body.set_draw_func({
        let ctx = ctx.clone();
        let share = body_share.clone();
        let (raised, panel, border) = (ctx.cfg.color("panel_raised"), ctx.cfg.color("panel"), ctx.cfg.color("border"));
        let strength = ctx.cfg.number("look.panel_opacity", 1.0);
        move |_, cr, _, _| {
            use std::f64::consts::{FRAC_PI_2, PI};
            let k = ctx.k;
            let p = share.get();
            if p <= 0.001 {
                return;
            }
            // draw in screen coordinates; the box only limits what is painted
            cr.translate(-ctx.screen_x(BODY_BOX.0).round(), -(BODY_BOX.1 * k + ctx.oy).round());
            let (w, h) = (width, height);
            // ease out with a slight overshoot, so the panel settles rather than stops
            let t = p - 1.0;
            let grow = 1.0 + 2.2 * t.powi(3) + 1.2 * t.powi(2);
            cr.push_group();
            // design pixels: the lobe (centre, radius), the body's box and corners, the fillet
            let (dx, dy) = (ctx.screen_x(0.0), ctx.oy);
            let (cx, cy, r) = (116.0 * k + dx, 551.0 * k + dy, 62.0 * k);
            let (x0, y0, x1, y1) = (152.0 * k + dx, 422.0 * k + dy, 420.0 * k + dx, 704.0 * k + dy);
            let (tl, tr, br, bl) = (40.0 * k, 60.0 * k, 66.0 * k, 60.0 * k);
            let f = 26.0 * k;
            let fx = x0 - f;
            let reach = ((r + f).powi(2) - (fx - cx).powi(2)).max(0.0).sqrt();
            let (up, low) = (cy - reach, cy + reach);
            // the outline is built scaled about the button; what fills it is not
            let _ = cr.save();
            cr.translate(cx, cy);
            cr.scale(0.25 + 0.75 * grow, 0.25 + 0.75 * grow);
            cr.translate(-cx, -cy);
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
            let _ = cr.restore();
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
            let _ = cr.pop_group_to_source();
            let _ = cr.paint_with_alpha((p * 1.8).min(1.0));
        }
    });
    ctx.put(&overlay, &body, BODY_BOX.0, BODY_BOX.1);
    // shut, only the round button is left: a clean circle
    let body_motion: Rc<RefCell<Option<gtk::TickCallbackId>>> = Rc::new(RefCell::new(None));
    let move_body = {
        let (body, share, running) = (body.clone(), body_share.clone(), body_motion.clone());
        Rc::new(move |open: bool| {
            if let Some(id) = running.borrow_mut().take() {
                id.remove();
            }
            let (from, to) = (share.get(), if open { 1.0 } else { 0.0 });
            let length = if open { 240.0 } else { 150.0 } * (to - from).abs();
            if !motion || length < 1.0 {
                share.set(to);
                body.queue_draw();
                return;
            }
            // one step per screen refresh, timed by the frame clock (a 16 ms timer drifts against
            // the display and stutters)
            let started: Cell<Option<i64>> = Cell::new(None);
            let (share2, running2) = (share.clone(), running.clone());
            let id = body.add_tick_callback(move |body2, clock| {
                let now = clock.frame_time();
                let begin = *started.get().get_or_insert(now);
                started.set(Some(begin));
                let t = ((now - begin) as f64 / 1000.0 / length).min(1.0);
                share2.set(from + (to - from) * t);
                body2.queue_draw();
                if t >= 1.0 {
                    running2.borrow_mut().take();
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
            *running.borrow_mut() = Some(id);
        })
    };
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

    let slots = [(190.0, 442.0, 196.0), (180.0, 490.0, 196.0), (184.0, 539.0, 196.0), (198.0, 588.0, 196.0), (216.0, 637.0, 196.0)];
    let main_pills: Vec<gtk::Button> = [
        // what the ring does not already offer: every app, recent files, the desktop's own
        // settings, the tips again, and the power choices
        ("squares-four-fill", "Applications", false),
        ("clock-counter-clockwise-bold", "Recent files", false),
        ("sliders-horizontal-bold", "Quick settings", false),
        ("question-bold", "Tips", false),
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
            let icon = match *name {
                "Lock" => "lock-bold",
                "Log out" => "sign-out-bold",
                "Restart" => "arrow-clockwise-bold",
                "Back" => "arrow-left-bold",
                _ => "power-bold",
            };
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
        let (parts, open, refrost, move_body) = (menu_parts, menu_open.clone(), refrost.clone(), move_body.clone());
        Rc::new(move |on: bool| {
            let on = on || always_open;
            if on != open.get() || on {
                move_body(on);
            }
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
    // The sheet (search, quick settings) is made with the top strip, further down.
    let open_sheet: Rc<RefCell<Option<Rc<panels::Shell>>>> = Rc::new(RefCell::new(None));
    for (pill, sheet) in main_pills.iter().zip(["search", "", "quick"]) {
        let (set_menu, open_sheet, recent) = (set_menu.clone(), open_sheet.clone(), cfg.commands("recent"));
        pill.connect_clicked(move |_| match open_sheet.borrow().as_ref() {
            // the panel opens beside the menu, so the menu stays open next to it
            Some(shell) if !sheet.is_empty() => shell.show_beside_menu(sheet),
            _ => {
                set_menu(false);
                run(&recent);
            }
        });
    }
    main_pills[3].connect_clicked({
        let (set_menu, tip_slot) = (set_menu.clone(), tip_slot.clone());
        move |_| {
            set_menu(false);
            if let Some(tip) = tip_slot.borrow().as_ref() {
                tip.set_visible(true);
            }
        }
    });
    // Restart, Shut down and Log out wait five seconds, counting on the pill, and "Back" reads
    // "Cancel" meanwhile; a click on either, Escape or a click elsewhere stops them.
    let pending: Rc<RefCell<Option<(glib::SourceId, gtk::Label, String)>>> = Rc::new(RefCell::new(None));
    let back_label = power_pills[4].child().and_then(|row| row.last_child()).and_downcast::<gtk::Label>();
    let cancel_pending = {
        let (pending, back_label) = (pending.clone(), back_label.clone());
        Rc::new(move || {
            if let Some((id, label, text)) = pending.borrow_mut().take() {
                id.remove();
                label.set_text(&text);
            }
            if let Some(back) = &back_label {
                back.set_text("Back");
            }
        })
    };
    let swap = {
        let (main, power, refrost, cancel_pending) = (main_pills.clone(), power_pills.clone(), refrost.clone(), cancel_pending.clone());
        move |show_power: bool| {
            cancel_pending();
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
        let (swap, set_menu, pending, cancel_pending, back_label) =
            (swap.clone(), set_menu.clone(), pending.clone(), cancel_pending.clone(), back_label.clone());
        // these are run as written, not looked up: they are shell lines, not program names
        let command = match cfg.get(&format!("commands.{key}")) {
            "${ARKAWM_LOCKER:-swaylock}" => LOCK.to_string(),
            other => other.to_string(),
        };
        let doing = match key {
            "restart" => "Restart",
            "shut_down" => "Shut down",
            _ => "Log out",
        };
        pill.connect_clicked(move |pill| {
            if key == "lock" {
                swap(false);
                set_menu(false);
                let _ = x11::shell(&command).spawn();
                return;
            }
            let this_one = pending.borrow().as_ref().is_some_and(|(_, label, _)| label.is_ancestor(pill));
            cancel_pending();
            if this_one {
                return;
            }
            let Some(label) = pill.child().and_then(|row| row.last_child()).and_downcast::<gtk::Label>() else { return };
            let original = label.text().to_string();
            label.set_text(&format!("{doing} in 5"));
            if let Some(back) = &back_label {
                back.set_text("Cancel");
            }
            let left = Rc::new(Cell::new(5u32));
            let id = glib::timeout_add_seconds_local(1, {
                let (swap, set_menu, pending, label, original, command) =
                    (swap.clone(), set_menu.clone(), pending.clone(), label.clone(), original.clone(), command.clone());
                move || {
                    let n = left.get() - 1;
                    left.set(n);
                    if n > 0 {
                        label.set_text(&format!("{doing} in {n}"));
                        return glib::ControlFlow::Continue;
                    }
                    pending.borrow_mut().take();
                    label.set_text(&original);
                    swap(false);
                    set_menu(false);
                    let _ = x11::shell(&command).spawn();
                    glib::ControlFlow::Break
                }
            });
            *pending.borrow_mut() = Some((id, label, original));
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
    // Each card can be closed with its ×; which ones are closed is remembered, and the
    // "Desktop cards" switch in quick settings brings them all back.
    let closed_cards = std::rc::Rc::new(RefCell::new(panels::closed_cards()));
    let card = |title: &str, gap: f64, id: &'static str| {
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
        let close = gtk::Button::new();
        close.set_child(Some(&ctx.icon("x-bold", 12.0)));
        close.add_css_class("cardclose");
        close.set_valign(Align::Center);
        close.set_tooltip_text(Some("Close this card (quick settings brings it back)"));
        close.update_property(&[gtk::accessible::Property::Label(&format!("Close {title}"))]);
        close.connect_clicked({
            let (column, closed, refrost) = (column.clone(), closed_cards.clone(), refrost.clone());
            move |_| {
                column.add_css_class("gone");
                closed.borrow_mut().push(id.to_string());
                panels::save_closed_cards(&closed.borrow());
                let (column, refrost) = (column.clone(), refrost.clone());
                glib::timeout_add_local_once(std::time::Duration::from_millis(if motion { 260 } else { 0 }), move || {
                    column.set_visible(false);
                    refrost();
                });
            }
        });
        if closed_cards.borrow().iter().any(|closed| closed == id) {
            column.set_visible(false);
        }
        (column, head, close)
    };

    let (privacy_card, privacy_head, privacy_close) = card("Privacy Status", 5.0, "privacy");
    let count = gtk::Label::new(Some(""));
    count.add_css_class("title");
    privacy_head.append(&count);
    privacy_head.append(&privacy_close);
    let mut privacy_rows = Vec::new();
    let mut privacy_lines = Vec::new();
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
        privacy_lines.push(row);
    }
    let privacy_note = gtk::Label::new(Some(if arkad_installed() {
        "The ArkaOS privacy service is installed but not running, so these settings cannot be checked. Start it with: sudo systemctl start arkad"
    } else {
        "The ArkaOS privacy service is not installed on this computer, so these settings cannot be checked here."
    }
    ));
    privacy_note.add_css_class("dim");
    privacy_note.set_wrap(true);
    privacy_note.set_xalign(0.0);
    privacy_note.set_max_width_chars(34);
    privacy_note.set_visible(false);
    privacy_card.append(&privacy_note);
    let checked_row = gtk::Box::new(Orientation::Horizontal, ctx.px(12.0));
    checked_row.set_margin_top(ctx.px(4.0));
    checked_row.append(&ctx.icon("arrows-clockwise-bold", 18.0));
    let checked = gtk::Label::new(Some("Not checked yet"));
    checked.add_css_class("dim");
    checked_row.append(&checked);
    privacy_card.append(&checked_row);
    aside.append(&privacy_card);

    let (health_card, health_head, health_close) = card("System Health", 11.0, "health");
    health_head.append(&health_close);
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

    let (updates_card, updates_head, updates_close) = card("Updates", 6.0, "updates");
    updates_head.append(&updates_close);
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
        open.set_visible(on_image());
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
        // The cards keep to the right edge of the screen. They move when dragged anywhere but on a
        // button, and get wider or narrower when dragged by their left edge; where they sit is remembered (see
        // `load_cards`). A double-click on a title puts them back.
        let layout = Rc::new(Cell::new(load_cards().unwrap_or(CARDS)));
        let apply = {
            let (aside, ctx) = (aside.clone(), ctx.clone());
            Rc::new(move |(right, top, wide): (f64, f64, f64)| {
                aside.set_size_request(ctx.px(wide), -1);
                aside.set_margin_end(ctx.px(right));
                aside.set_margin_top(ctx.px(top) + ctx.oy.round() as i32);
            })
        };
        aside.set_halign(Align::End);
        aside.set_valign(Align::Start);
        aside.set_cursor_from_name(Some("grab"));
        apply(layout.get());
        overlay.add_overlay(&aside);
        let heads: Rc<Vec<gtk::Widget>> = Rc::new(vec![privacy_head.clone().upcast(), health_head.clone().upcast(), updates_head.clone().upcast()]);
        for head in heads.iter() {
            head.set_tooltip_text(Some("Drag to move the cards, or drag their left edge to resize them. Double-click a title to put them back."));
            let again = gtk::GestureClick::new();
            again.connect_pressed({
                let (layout, apply, frost) = (layout.clone(), apply.clone(), frost.clone());
                move |_, presses, _, _| {
                    if presses == 2 {
                        layout.set(CARDS);
                        apply(CARDS);
                        save_cards(None);
                        frost.queue_draw();
                    }
                }
            });
            head.add_controller(again);
        }
        // How close to the cards' left edge, in screen pixels, a press resizes them.
        let grip = ctx.px(8.0) as f32;
        let near_edge = {
            let (aside, overlay) = (aside.clone(), overlay.clone());
            move |x: f64, y: f64| {
                aside.is_visible()
                    && aside.compute_bounds(&overlay).is_some_and(|b| {
                        b.width() > 0.0 && (x as f32 - b.x()).abs() <= grip && y as f32 >= b.y() && y as f32 <= b.y() + b.height()
                    })
            }
        };
        // 0: no drag, 1: moving, 2: resizing
        let mode = Rc::new(Cell::new(0u8));
        let from = Rc::new(Cell::new(CARDS));
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin({
            let (overlay, aside, mode, from, layout, near_edge) = (overlay.clone(), aside.clone(), mode.clone(), from.clone(), layout.clone(), near_edge.clone());
            move |gesture, x, y| {
                // on a card, but not on one of its buttons (close, open updates)
                let on_card = overlay.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|picked| {
                    (picked == *aside.upcast_ref::<gtk::Widget>() || picked.is_ancestor(&aside))
                        && picked.ancestor(gtk::Button::static_type()).is_none_or(|button| !button.is_ancestor(&aside))
                });
                mode.set(if near_edge(x, y) { 2 } else if on_card { 1 } else { 0 });
                if mode.get() == 0 {
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
                from.set(layout.get());
            }
        });
        drag.connect_drag_update({
            let (mode, from, layout, apply, frost) = (mode.clone(), from.clone(), layout.clone(), apply.clone(), frost.clone());
            let (screen_w, screen_h) = (width as f64 / k, height as f64 / k);
            move |_, dx, dy| {
                let (right, top, wide) = from.get();
                let (dx, dy) = (dx / k, dy / k);
                let next = match mode.get() {
                    1 => ((right - dx).clamp(0.0, (screen_w - wide).max(0.0)), (top + dy).clamp(0.0, (screen_h - 120.0).max(0.0)), wide),
                    2 => (right, top, (wide - dx).clamp(CARDS_NARROWEST, CARDS_WIDEST).min(screen_w - right)),
                    _ => return,
                };
                layout.set(next);
                apply(next);
                frost.queue_draw();
            }
        });
        drag.connect_drag_end({
            let (mode, layout) = (mode.clone(), layout.clone());
            move |_, _, _| {
                if mode.replace(0) != 0 {
                    save_cards(Some(layout.get()));
                }
            }
        });
        overlay.add_controller(drag);
        // the pointer shows when the edge can be dragged
        let hover = gtk::EventControllerMotion::new();
        hover.connect_motion({
            let (overlay, aside) = (overlay.clone(), aside.clone());
            move |_, x, y| {
                let edge = near_edge(x, y);
                overlay.set_cursor_from_name(if edge { Some("ew-resize") } else { None });
                aside.set_cursor_from_name(Some(if edge { "ew-resize" } else { "grab" }));
            }
        });
        overlay.add_controller(hover);
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
    // Windows get the whole screen: the strip shows on the bare desktop and, while a window is
    // up, tucks itself away until the pointer touches the top edge (see below).
    strip.set_exclusive_zone(0);
    strip.set_default_size(width, ctx.px(60.0));
    let bar = gtk::CenterBox::new();
    bar.set_margin_start(ctx.px(34.0));
    bar.set_margin_end(ctx.px(30.0));
    bar.set_margin_top(ctx.px(11.0));
    bar.set_margin_bottom(ctx.px(11.0));
    let brand_row = gtk::Box::new(Orientation::Horizontal, ctx.px(14.0));
    brand_row.append(&ctx.logo(36.0));
    if let Some(text) = text("brand") {
        let name = gtk::Label::new(Some(text));
        name.add_css_class("brand");
        brand_row.append(&name);
    }
    // the mark in the strip shows the desktop (the big mark folds the apps instead)
    let brand = gtk::Button::new();
    brand.add_css_class("brandbutton");
    brand.set_child(Some(&brand_row));
    brand.set_tooltip_text(Some("Show the desktop"));
    brand.update_property(&[gtk::accessible::Property::Label("Show the desktop")]);
    bar.set_start_widget(Some(&brand));

    // Open windows in the middle; notifications, quick settings and clipboard history on the right.
    let shell = panels::Shell::new(app, ctx.clone());
    *open_sheet.borrow_mut() = Some(shell.clone());
    brand.connect_clicked({
        let (shell, swap, set_menu) = (shell.clone(), swap.clone(), set_menu.clone());
        move |_| {
            swap(false);
            set_menu(false);
            shell.show_desktop();
        }
    });
    mark.connect_clicked({
        let (swap, set_menu, refrost, ring) = (swap.clone(), set_menu.clone(), refrost.clone(), ring.clone());
        let folded = Cell::new(false);
        let wind = wind.clone();
        move |mark| {
            swap(false);
            set_menu(false);
            let fold = !folded.get();
            folded.set(fold);
            for node in &nodes {
                if fold {
                    node.add_css_class("folded");
                } else {
                    node.remove_css_class("folded");
                }
                node.set_can_target(!fold);
                node.set_can_focus(!fold);
            }
            if fold {
                ring.add_css_class("folded");
            } else {
                ring.remove_css_class("folded");
            }
            // wind the ring back, or draw it round again, in step with the apps (style.css)
            wind(if fold { 0.0 } else { 1.0 }, if fold { 300.0 } else { 440.0 });
            let tip = if fold { "Show the apps" } else { "Hide the apps" };
            mark.set_tooltip_text(Some(tip));
            mark.update_property(&[gtk::accessible::Property::Label(tip)]);
            // The glass moves with the apps every frame until they have landed (the GPU draws it).
            refrost();
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
    let me = shell.clone();
    search.connect_clicked(move |_| me.toggle("search"));
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
    bar.add_css_class("bar");
    strip.set_child(Some(&bar));
    strip.present();

    // Tucked away, the strip is taken down and a thin line stays along the middle third of the top
    // edge. The pointer resting on that line for a moment brings the strip back; leaving the strip
    // (with nothing of it open) sends it up again. The corners hold nothing: that is where a
    // window's tabs and close button are, and the strip must not drop over them on the way there.
    let line = layer_window(app, "edgeline", Layer::Top);
    line.set_anchor(Edge::Left, false);
    line.set_anchor(Edge::Right, false);
    line.set_exclusive_zone(0);
    line.set_default_size(width / 3, 3);
    // an empty window shrinks to nothing; this keeps the line 3 pixels high
    let line_body = gtk::Box::new(Orientation::Horizontal, 0);
    line_body.set_size_request(width / 3, 3);
    line.set_child(Some(&line_body));
    let tucked = Rc::new(Cell::new(false));
    let pointer_in = Rc::new(Cell::new(false));
    let windows_up = Rc::new(Cell::new(false));
    let tuck = {
        let (strip, bar, tucked, line) = (strip.clone(), bar.clone(), tucked.clone(), line.clone());
        Rc::new(move |away: bool| {
            if away == tucked.get() {
                return;
            }
            tucked.set(away);
            if away {
                // slide up and fade, then take the strip down and leave the line
                bar.add_css_class("tucked");
                let (strip, tucked, line) = (strip.clone(), tucked.clone(), line.clone());
                glib::timeout_add_local_once(std::time::Duration::from_millis(if motion { 260 } else { 0 }), move || {
                    if tucked.get() {
                        strip.set_visible(false);
                        line.present();
                    }
                });
            } else {
                line.set_visible(false);
                if strip.is_visible() || !motion {
                    strip.present();
                    bar.remove_css_class("tucked");
                    return;
                }
                // put it back still tucked, and let it slide down once it is on screen
                strip.present();
                let (bar, frames) = (bar.clone(), Cell::new(0));
                strip.add_tick_callback(move |_, _| {
                    frames.set(frames.get() + 1);
                    if frames.get() < 2 {
                        return glib::ControlFlow::Continue;
                    }
                    bar.remove_css_class("tucked");
                    glib::ControlFlow::Break
                });
            }
        })
    };
    // put the strip away once the pointer has left it and nothing of it is open
    let tuck_later = {
        let (tuck, pointer_in, windows_up, shell) = (tuck.clone(), pointer_in.clone(), windows_up.clone(), shell.clone());
        Rc::new(move || {
            let (tuck, pointer_in, windows_up, shell) = (tuck.clone(), pointer_in.clone(), windows_up.clone(), shell.clone());
            glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                if pointer_in.get() || !windows_up.get() {
                    return glib::ControlFlow::Break;
                }
                if shell.sheet_open() {
                    return glib::ControlFlow::Continue;
                }
                tuck(true);
                glib::ControlFlow::Break
            });
        })
    };
    let hover = gtk::EventControllerMotion::new();
    hover.connect_enter({
        let (tuck, pointer_in) = (tuck.clone(), pointer_in.clone());
        move |_, _, _| {
            pointer_in.set(true);
            // back on the strip while it slides away: it comes straight back
            tuck(false);
        }
    });
    hover.connect_leave({
        let (pointer_in, tuck_later) = (pointer_in.clone(), tuck_later.clone());
        move |_| {
            pointer_in.set(false);
            tuck_later();
        }
    });
    strip.add_controller(hover);
    // the line: only a pointer that stays on it for 0.6 s brings the strip down
    let on_line = Rc::new(Cell::new(false));
    let line_hover = gtk::EventControllerMotion::new();
    line_hover.connect_enter({
        let (tuck, tuck_later, on_line) = (tuck.clone(), tuck_later.clone(), on_line.clone());
        let turn = Rc::new(Cell::new(0u32));
        move |_, _, _| {
            on_line.set(true);
            turn.set(turn.get() + 1);
            let (tuck, tuck_later, on_line, turn, mine) = (tuck.clone(), tuck_later.clone(), on_line.clone(), turn.clone(), turn.get());
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                if on_line.get() && turn.get() == mine {
                    tuck(false);
                    // if the pointer has moved off by the time the strip is down, it goes up again
                    tuck_later();
                }
            });
        }
    });
    line_hover.connect_leave({
        let on_line = on_line.clone();
        move |_| on_line.set(false)
    });
    line.add_controller(line_hover);
    let window_count = Cell::new(0usize);
    shell.watch_windows({
        let (tuck, windows_up, tuck_later, pointer_in, ctx) = (tuck.clone(), windows_up.clone(), tuck_later.clone(), pointer_in.clone(), ctx.clone());
        move |count, up| {
            ctx.covered.set(up);
            // a new window: the app that was clicked has started
            if count > window_count.get() {
                started();
            }
            window_count.set(count);
            windows_up.set(up);
            if !up {
                tuck(false);
            } else if !pointer_in.get() {
                tuck_later();
            }
        }
    });
    // Double-click the bare wallpaper: the apps and the cards step aside so the whole picture
    // shows (on a small laptop screen they cover much of it). Again brings them back.
    {
        let clear = gtk::GestureClick::new();
        clear.set_button(1);
        let (base, mark, aside, ring, refrost) = (overlay.clone(), mark.clone(), aside.clone(), ring.clone(), refrost.clone());
        let cleared = Cell::new(false);
        clear.connect_pressed(move |gesture, presses, x, y| {
            if presses != 2 {
                return;
            }
            let (Some(picked), Some(picture)) = (base.pick(x, y, gtk::PickFlags::DEFAULT), base.child()) else { return };
            if !(picked == picture || picked.is_ancestor(&picture) || picked == *base.upcast_ref::<gtk::Widget>()) {
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let on = !cleared.get();
            cleared.set(on);
            if show_ring && on != ring.has_css_class("folded") {
                mark.emit_clicked();
            }
            // the logo in the middle steps aside too: the whole picture, nothing on top
            for part in [aside.upcast_ref::<gtk::Widget>(), mark.upcast_ref()] {
                if on {
                    part.add_css_class("cleared");
                } else {
                    part.remove_css_class("cleared");
                }
                part.set_can_target(!on);
            }
            refrost();
        });
        overlay.add_controller(clear);
    }
    panels::serve_notifications(&shell);
    shell.start();

    // ───── Live values ─────
    let live = Rc::new(Live { pill_dot, pill_text, date, clock, count, privacy_rows, privacy_lines, privacy_note, signature_row, checked, health, updates, signature });

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
    let (send, receive) = mpsc::channel::<(bool, Privacy)>();
    std::thread::spawn(move || loop {
        let running = arkad_running();
        let privacy = read_privacy();
        if send.send((running, privacy)).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(30));
    });

    let mut last_cpu = cpu_times();
    let icons = ctx.clone();
    let mut checked_at: Option<std::time::Instant> = None;
    let mut service_found = true;
    let glass = frost.clone();
    let mut ticks = 0u32;
    let tick = move || {
        while let Ok((running, privacy)) = receive.try_recv() {
            checked_at = Some(std::time::Instant::now());
            service_found = running;
            // Without the service, six "Unavailable" rows read as six failures. One line says why.
            let resized = live.privacy_note.is_visible() == running || live.signature_row.is_visible() != on_image();
            for line in &live.privacy_lines {
                line.set_visible(running);
            }
            live.privacy_note.set_visible(!running);
            live.signature_row.set_visible(on_image());
            if resized {
                // The glass behind the cards follows their new height once they are laid out again.
                let glass = glass.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || glass.queue_draw());
            }
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
            if !running {
                live.count.set_text("");
                live.pill_text.set_text("Privacy service not running");
                live.pill_dot.add_css_class("off");
            } else if known == 0 {
                live.count.set_text("");
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
            icons.show_verified(running && good == 6);
            live.updates.set_text(match privacy[5] {
                Some(true) => "Automatic updates on",
                Some(false) => "Automatic updates off",
                None if !on_image() => "ArkaOS updates do not apply here",
                None => "Update status unavailable",
            });
            // what the policy file says, not a claim that this system was verified
            live.signature.set_text(match signature_policy() {
                Some(true) => "Signatures required",
                Some(false) => "Not required",
                None => "Unavailable",
            });
        }
        // without the service nothing was checked, only looked for
        let what = if service_found { "Last checked" } else { "Looked for the service" };
        live.checked.set_text(&match checked_at.map(|at| at.elapsed().as_secs()) {
            None => "Not checked yet".to_string(),
            // whole minutes: a number counting up every second is motion on a resting screen
            Some(s) if s < 60 => format!("{what}: just now"),
            Some(s) if s < 120 => format!("{what}: 1 min ago"),
            Some(s) => format!("{what}: {} min ago", s / 60),
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
    x11::hide();
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
    use super::{next_frame, plain_icon, scale_css, Grade};

    #[test]
    fn green_icons_are_plain_until_the_privacy_settings_were_read() {
        assert_eq!(plain_icon("shield-green"), "shield-fill");
        assert_eq!(plain_icon("globe-bold"), "globe-bold");
        assert_eq!(plain_icon("app:firefox"), "app:firefox");
    }

    #[test]
    fn picked_apps_get_the_theme_glyph_for_their_kind() {
        assert_eq!(super::glyph_for("Network;WebBrowser;"), Some("globe-bold"));
        assert_eq!(super::glyph_for("Qt;KDE;System;TerminalEmulator;"), Some("terminal-bold"));
        assert_eq!(super::glyph_for("Utility;TextEditor;"), Some("code-bold"));
        assert_eq!(super::glyph_for("Network;Feed;"), None);
        assert_eq!(super::glyph_for(""), None);
    }

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
