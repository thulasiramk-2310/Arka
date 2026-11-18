//! arka-lock: the ArkaWM lock screen.
//!
//! Locks through ext-session-lock (gtk4-layer-shell's session lock), so the compositor keeps the
//! screen locked even if this program crashes. Every screen shows the wallpaper, the time and the
//! date; the first one also shows who is signed in and the password box. The password is checked
//! with PAM, off the main thread, so the clock keeps ticking while PAM makes a wrong guess wait.
//!
//! The look comes from the desktop shell: arka-desk writes the current wallpaper, font and colours
//! to `$XDG_RUNTIME_DIR/arka-desk-lock.conf`. Without that file the Sky Blue defaults are used.
//!
//! `arka-lock` returns when the screen is unlocked. `arka-lock -f` returns as soon as the screen
//! is locked (for swayidle's before-sleep), and keeps the lock screen running in the background.
//!
//! `arka-lock --greeter` is the sign-in screen instead: the same card, started by greetd before
//! anyone has signed in (see greet.rs).

mod greet;
mod greetd;
mod pam;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_session_lock::Instance as Lock;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;

/// Set in the background copy started by `-f`, which reports "locked" on its standard output.
const CHILD: &str = "ARKA_LOCK_REPORT";

fn main() -> glib::ExitCode {
    // GTK's graphics setup opens the X11 display when it is set, and on ArkaWM that starts
    // Xwayland at the first idle lock. The lock screen starts no X11 programs; it needs none.
    std::env::remove_var("DISPLAY");
    if std::env::args().skip(1).any(|a| a == "--greeter") {
        return greet::main();
    }
    let daemonize = std::env::args().skip(1).any(|a| a == "-f" || a == "--daemonize");
    if daemonize && std::env::var_os(CHILD).is_none() {
        return wait_for_lock();
    }
    let app = gtk::Application::builder()
        .application_id("org.arka.Lock")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let failed = Rc::new(Cell::new(false));
    let failed_too = failed.clone();
    app.connect_activate(move |app| start(app, failed_too.clone()));
    // GTK would read our own flags as its own; it gets none.
    let code = app.run_with_args::<&str>(&[]);
    if failed.get() { glib::ExitCode::FAILURE } else { code }
}

/// `-f`: start the real lock screen as a child and come back once it says the screen is locked
/// (or once it ends, when the screen could not be locked).
fn wait_for_lock() -> glib::ExitCode {
    let Ok(me) = std::env::current_exe() else { return glib::ExitCode::FAILURE };
    let child = std::process::Command::new(me)
        .env(CHILD, "1")
        .stdout(std::process::Stdio::piped())
        .spawn();
    let Ok(mut child) = child else { return glib::ExitCode::FAILURE };
    let mut line = String::new();
    if let Some(out) = child.stdout.take() {
        let _ = std::io::BufReader::new(out).read_line(&mut line);
    }
    if line.trim() == "locked" { glib::ExitCode::SUCCESS } else { glib::ExitCode::FAILURE }
}

/// The desktop's look, as arka-desk wrote it.
struct Look {
    values: HashMap<String, String>,
}

impl Look {
    fn read() -> Look {
        let mut values: HashMap<String, String> = [
            ("background", "#101820"),
            ("panel", "#18232F"),
            ("border", "#65758A"),
            ("text", "#EDF3FA"),
            ("text_dim", "#A8B6C7"),
            ("accent", "#76CFFF"),
            ("warn", "#FFC98A"),
            ("font", "Inter"),
            ("wallpaper", "/usr/share/arka-desk/sky-blue/wallpaper.jpg"),
            ("icons", "/usr/share/arka-desk/sky-blue"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            if let Ok(text) = std::fs::read_to_string(PathBuf::from(dir).join("arka-desk-lock.conf")) {
                values.extend(parse(&text));
            }
        }
        Look { values }
    }

    /// One of the theme's icons, as a picture for a button or label.
    fn icon(&self, name: &str, size: i32) -> gtk::Image {
        let image = gtk::Image::from_file(PathBuf::from(self.get("icons")).join(format!("{name}.svg")));
        image.set_pixel_size(size);
        image
    }

    fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }

    /// A colour as CSS rgba() with this opacity, from #RRGGBB.
    fn rgba(&self, key: &str, alpha: f64) -> String {
        let hex = self.get(key).trim_start_matches('#');
        let part = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(128);
        format!("rgba({}, {}, {}, {alpha})", part(0), part(2), part(4))
    }

    fn css(&self) -> String {
        format!(
            r#"
            window.lock {{ background: {bg}; font-family: "{font}", "Noto Sans", sans-serif; color: {text}; }}
            /* darker at the top for the clock and at the bottom for the status row; clear between */
            .veil {{ background: linear-gradient(180deg, {veil_top} 0%, {veil_clear} 34%, {veil_clear} 62%, {veil_low} 100%); }}

            .time {{ font-size: 120px; font-weight: 200; letter-spacing: -3px; font-feature-settings: "tnum";
                     text-shadow: 0 2px 28px {shade}; }}
            .date {{ font-size: 18px; font-weight: 600; color: {text}; text-shadow: 0 1px 3px {shade}, 0 0 18px {shade}; }}

            /* the card: the softened wallpaper (drawn in code) under a tint, a hairline and a soft drop */
            .card {{ border-radius: 28px; border: 1px solid {hair};
                     box-shadow: 0 28px 64px {drop}, 0 2px 8px {drop_near}; }}
            .cardbody {{ padding: 30px 34px 24px 34px;
                         background: linear-gradient(165deg, {tint_top}, {tint_low});
                         box-shadow: inset 0 1px 0 {inner}; }}
            .avatar {{ min-width: 72px; min-height: 72px; border-radius: 999px; background: {accent_wash};
                       border: 1px solid {accent_line}; color: {accent}; font-size: 30px; font-weight: 600; }}
            .name {{ font-size: 18px; font-weight: 600; }}
            .field {{ background: {well}; border: 1px solid {hair}; border-radius: 999px; padding: 4px 5px 4px 18px;
                      transition: border-color 180ms cubic-bezier(0.16,1,0.3,1), box-shadow 220ms cubic-bezier(0.16,1,0.3,1),
                                  opacity 180ms cubic-bezier(0.16,1,0.3,1); }}
            .field.focused {{ border-color: {accent}; box-shadow: 0 0 0 3px {accent_ring}; }}
            .field.wrong {{ border-color: {warn}; box-shadow: 0 0 0 3px {warn_ring}; }}
            .field.busy {{ opacity: 0.72; }}
            .field entry {{ background: none; border: none; box-shadow: none; outline: none; color: {text};
                            font-size: 15px; min-height: 38px; padding: 0; caret-color: {accent}; }}
            .field entry > text > placeholder {{ color: {dim}; }}
            .field entry > text > selection {{ background: {accent_ring}; color: {text}; }}
            .go {{ min-width: 38px; min-height: 38px; border-radius: 999px; padding: 0; background: {accent};
                   color: {bg}; border: none; box-shadow: none; transition: background-color 160ms, transform 160ms cubic-bezier(0.16,1,0.3,1); }}
            .go:hover {{ background: {accent_hover}; }}
            .go:active {{ transform: scale(0.94); }}
            .go:disabled {{ background: {well}; color: {dim}; }}
            .note {{ font-size: 13px; color: {dim}; min-height: 18px; }}
            .note.wrong {{ color: {warn}; }}
            .layout {{ font-size: 12px; color: {dim}; margin-top: 4px; }}

            .status {{ font-size: 13px; font-weight: 500; color: {text_soft}; font-feature-settings: "tnum"; text-shadow: 0 1px 10px {shade}; }}
            .power {{ min-width: 40px; min-height: 40px; border-radius: 999px; padding: 0; background: {glass};
                      border: 1px solid {hair}; box-shadow: 0 6px 18px {drop_near};
                      transition: background-color 160ms cubic-bezier(0.16,1,0.3,1), border-color 160ms cubic-bezier(0.16,1,0.3,1); }}
            .power:hover {{ background: {glass_hover}; border-color: {accent_line}; }}
            .power:focus-visible {{ border-color: {accent}; box-shadow: 0 0 0 3px {accent_ring}; }}
            .power.armed {{ border-color: {warn}; background: {warn_wash}; }}

            /* Arriving: the clock settles down, the card rises, the edges come last. One movement,
               eased out. "waiting" comes off once the first frame is on screen (see `arrive`), so
               the movement is seen from its start; GTK skips it when animations are turned off. */
            .clockblock {{ transition: opacity 560ms cubic-bezier(0.16,1,0.3,1), transform 640ms cubic-bezier(0.16,1,0.3,1); }}
            .card {{ transition: opacity 520ms cubic-bezier(0.16,1,0.3,1) 80ms, transform 640ms cubic-bezier(0.16,1,0.3,1) 80ms; }}
            .edge {{ transition: opacity 480ms cubic-bezier(0.16,1,0.3,1) 240ms; }}
            .waiting .clockblock {{ opacity: 0; transform: translateY(-10px); }}
            .waiting .card {{ opacity: 0; transform: translateY(18px) scale(0.985); }}
            .waiting .edge {{ opacity: 0; }}
            /* a wrong password: the box shakes its head, once */
            @keyframes shake {{ 0% {{ transform: translateX(0px); }} 14% {{ transform: translateX(-9px); }} 30% {{ transform: translateX(8px); }}
                               46% {{ transform: translateX(-6px); }} 62% {{ transform: translateX(4px); }} 78% {{ transform: translateX(-2px); }}
                               100% {{ transform: translateX(0px); }} }}
            .field.shake {{ animation: shake 440ms cubic-bezier(0.36,0.07,0.19,0.97); }}
            /* unlocked: the card and clock step back before the desktop shows */
            @keyframes leave {{ from {{ opacity: 1; transform: scale(1); }} to {{ opacity: 0; transform: scale(0.97); }} }}
            .leaving .card, .leaving .clockblock, .leaving .edge {{ animation: leave 180ms cubic-bezier(0.4,0,1,1) forwards; }}
            "#,
            bg = self.get("background"),
            veil_top = self.rgba("background", 0.50),
            veil_clear = self.rgba("background", 0.10),
            veil_low = self.rgba("background", 0.50),
            shade = self.rgba("background", 0.45),
            font = self.get("font"),
            text = self.get("text"),
            text_soft = self.rgba("text", 0.88),
            dim = self.get("text_dim"),
            hair = self.rgba("text", 0.16),
            inner = self.rgba("text", 0.10),
            drop = self.rgba("background", 0.42),
            drop_near = self.rgba("background", 0.28),
            tint_top = self.rgba("panel", 0.46),
            tint_low = self.rgba("panel", 0.66),
            well = self.rgba("background", 0.40),
            glass = self.rgba("panel", 0.52),
            glass_hover = self.rgba("panel", 0.72),
            accent = self.get("accent"),
            accent_wash = self.rgba("accent", 0.14),
            accent_line = self.rgba("accent", 0.45),
            accent_ring = self.rgba("accent", 0.28),
            accent_hover = self.rgba("accent", 0.85),
            warn = self.get("warn"),
            warn_ring = self.rgba("warn", 0.22),
            warn_wash = self.rgba("warn", 0.14),
        )
    }
}

/// A small, softened copy of the wallpaper, painted stretched behind the card so it reads as
/// frosted glass (GTK has no backdrop blur). Shrinking to about 1/28 and stretching back spreads
/// each point that far.
fn softened(path: &str) -> Option<gtk::cairo::ImageSurface> {
    use gtk::gdk::prelude::GdkCairoContextExt;
    let full = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, 1920, -1, true).ok()?;
    let (w, h) = ((full.width() / 28).max(8), (full.height() / 28).max(8));
    let small = full.scale_simple(w, h, gtk::gdk_pixbuf::InterpType::Bilinear)?;
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, w, h).ok()?;
    let cr = gtk::cairo::Context::new(&surface).ok()?;
    cr.set_source_pixbuf(&small, 0.0, 0.0);
    cr.paint().ok()?;
    drop(cr);
    Some(surface)
}

/// `key = value` lines; `#` starts a comment.
fn parse(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// The signed-in user: login name for PAM, and the name to show (the full name when set).
struct Person {
    login: String,
    shown: String,
}

fn person() -> Person {
    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).and_then(|l| l.split_whitespace().nth(1).map(String::from)));
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let entry = uid.and_then(|uid| passwd.lines().find(|l| l.split(':').nth(2) == Some(uid.as_str())).map(String::from));
    let login = entry
        .as_deref()
        .and_then(|e| e.split(':').next())
        .map(String::from)
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_default();
    let full = entry.as_deref().and_then(|e| e.split(':').nth(4)).and_then(|g| g.split(',').next()).unwrap_or("").trim();
    let shown = if full.is_empty() { login.clone() } else { full.to_string() };
    Person { login, shown }
}

/// The letter in the round badge: the first letter of the shown name.
fn initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into())
}

fn now(format: &str) -> String {
    glib::DateTime::now_local().ok().and_then(|t| t.format(format).ok()).map(|s| s.to_string()).unwrap_or_default()
}

/// The password box and what goes with it, on the first screen only.
struct Form {
    /// The window the form is on: marked "leaving" for the short exit before unlocking.
    window: gtk::Window,
    /// The round badge and the name under it (the sign-in screen changes them for another user).
    avatar: gtk::Label,
    name: gtk::Label,
    /// The card's contents, bottom left battery: the sign-in screen adds and moves things here.
    body: gtk::Box,
    battery: gtk::Box,
    /// Paints the softened wallpaper behind the card.
    frost: gtk::DrawingArea,
    entry: gtk::PasswordEntry,
    go: gtk::Button,
    field: gtk::Box,
    note: gtk::Label,
    layout: gtk::Label,
    busy: Cell<bool>,
    /// Restart or Shut down waiting for its second click.
    armed: RefCell<Option<(gtk::Button, glib::SourceId)>>,
}

fn start(app: &gtk::Application, failed: Rc<Cell<bool>>) {
    if !gtk4_session_lock::is_supported() {
        eprintln!("arka-lock: this desktop does not offer screen locking (ext-session-lock)");
        failed.set(true);
        return;
    }
    let look = Rc::new(Look::read());
    // The frosted copy is made after the screens are up: the lock screen covers the desktop as
    // fast as it can, and the card is still fading in when the glass arrives.
    let soft: Rc<RefCell<Option<gtk::cairo::ImageSurface>>> = Rc::default();
    let frosts: Rc<RefCell<Vec<gtk::DrawingArea>>> = Rc::default();
    let css = gtk::CssProvider::new();
    css.load_from_string(&look.css());
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let lock = Lock::new();
    let person = Rc::new(person());
    let form: Rc<RefCell<Option<Rc<Form>>>> = Rc::default();
    let clocks: Rc<RefCell<Vec<(gtk::Label, gtk::Label)>>> = Rc::default();
    // keeps the app alive while the windows come and go
    let hold = app.hold();

    {
        let (app, look, person, form, clocks, soft, frosts) =
            (app.clone(), look.clone(), person.clone(), form.clone(), clocks.clone(), soft.clone(), frosts.clone());
        lock.connect_monitor(move |lock, monitor| {
            let with_form = form.borrow().is_none();
            let height = monitor.geometry().height();
            let (window, time, date, built) = screen(&app, &look, &person, &soft, height, with_form);
            if let Some(built) = &built {
                frosts.borrow_mut().push(built.frost.clone());
            }
            if let Some(built) = built {
                wire(lock, &built, &person);
                *form.borrow_mut() = Some(built);
            }
            clocks.borrow_mut().push((time, date));
            lock.assign_window_to_monitor(&window, monitor);
            window.present();
        });
    }
    lock.connect_locked(|_| {
        // `-f` is waiting for this line to return.
        if std::env::var_os(CHILD).is_some() {
            let mut out = std::io::stdout();
            let _ = writeln!(out, "locked");
            let _ = out.flush();
        }
    });
    {
        let failed = failed.clone();
        let app = app.clone();
        lock.connect_failed(move |_| {
            eprintln!("arka-lock: the screen could not be locked (another lock screen may be running)");
            failed.set(true);
            app.quit();
        });
    }
    {
        let app = app.clone();
        // The lock library takes its windows down during this signal; quit once it is done.
        lock.connect_unlocked(move |_| {
            let app = app.clone();
            glib::idle_add_local_once(move || app.quit());
        });
    }

    // The clock's timer also keeps the lock and the app's hold alive for as long as the app runs.
    let keep = (lock.clone(), hold);
    glib::timeout_add_seconds_local(1, move || {
        let _ = &keep;
        let (time, date) = (now("%H:%M"), now("%A, %-d %B"));
        for (t, d) in clocks.borrow().iter() {
            if t.text() != time {
                t.set_text(&time);
            }
            if d.text() != date {
                d.set_text(&date);
            }
        }
        glib::ControlFlow::Continue
    });

    {
        let (soft, frosts, look) = (soft.clone(), frosts.clone(), look.clone());
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            *soft.borrow_mut() = softened(look.get("wallpaper"));
            for frost in frosts.borrow().iter() {
                frost.queue_draw();
            }
        });
    }
    if !lock.lock() {
        eprintln!("arka-lock: the screen could not be locked");
        failed.set(true);
        app.quit();
    }
}

/// One screen's window: the wallpaper, the clock high up, and on the first screen the glass card
/// (who is signed in, the password box) with the battery and power buttons along the bottom.
/// A plain window: an application window crashed GTK when the lock library destroyed it at unlock.
fn screen(
    _app: &gtk::Application,
    look: &Look,
    person: &Person,
    soft: &Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    height: i32,
    with_form: bool,
) -> (gtk::Window, gtk::Label, gtk::Label, Option<Rc<Form>>) {
    let window = gtk::Window::builder().css_classes(["lock", "waiting"]).build();
    arrive(&window);
    let overlay = gtk::Overlay::new();

    let wallpaper = gtk::Picture::for_filename(look.get("wallpaper"));
    wallpaper.set_content_fit(gtk::ContentFit::Cover);
    wallpaper.set_can_shrink(true);
    overlay.set_child(Some(&wallpaper));
    let veil = gtk::Box::builder().css_classes(["veil"]).hexpand(true).vexpand(true).build();
    overlay.add_overlay(&veil);

    let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).halign(gtk::Align::Center).build();
    // the clock sits about a tenth of the way down, whatever the screen's height
    let clockblock = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["clockblock"])
        .margin_top((height as f64 * 0.10).round() as i32)
        .build();
    let time = gtk::Label::builder().label(now("%H:%M")).css_classes(["time"]).build();
    let date = gtk::Label::builder().label(now("%A, %-d %B")).css_classes(["date"]).build();
    clockblock.append(&time);
    clockblock.append(&date);
    column.append(&clockblock);

    let mut built = None;
    if with_form {
        let body = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["cardbody"]).build();
        let avatar = gtk::Label::builder().label(initial(&person.shown)).css_classes(["avatar"]).halign(gtk::Align::Center).build();
        avatar.set_accessible_role(gtk::AccessibleRole::Presentation);
        let name = gtk::Label::builder().label(&person.shown).css_classes(["name"]).margin_top(12).build();
        body.append(&avatar);
        body.append(&name);

        let entry = gtk::PasswordEntry::builder().show_peek_icon(true).hexpand(true).build();
        entry.set_placeholder_text(Some("Password"));
        entry.update_property(&[gtk::accessible::Property::Label("Password")]);
        let go = gtk::Button::builder().icon_name("go-next-symbolic").css_classes(["go"]).valign(gtk::Align::Center).build();
        go.set_tooltip_text(Some("Unlock"));
        go.update_property(&[gtk::accessible::Property::Label("Unlock")]);
        let field = gtk::Box::builder().spacing(8).css_classes(["field"]).width_request(312).margin_top(22).halign(gtk::Align::Center).build();
        field.append(&entry);
        field.append(&go);
        let note = gtk::Label::builder().css_classes(["note"]).margin_top(12).wrap(true).max_width_chars(40).justify(gtk::Justification::Center).build();
        note.set_accessible_role(gtk::AccessibleRole::Status);
        let layout = gtk::Label::builder().css_classes(["layout"]).visible(false).build();
        body.append(&field);
        body.append(&note);
        body.append(&layout);

        // the card: the softened wallpaper where the card is, the tinted body over it
        let card = gtk::Overlay::builder().css_classes(["card"]).halign(gtk::Align::Center).overflow(gtk::Overflow::Hidden).build();
        let frost = gtk::DrawingArea::new();
        frost.set_draw_func({
            let (soft, window) = (soft.clone(), window.clone());
            move |area, cr, _, _| {
                let soft = soft.borrow();
                let Some(surface) = soft.as_ref() else { return };
                let (ww, wh) = (window.width() as f64, window.height() as f64);
                let Some(at) = area.compute_point(&window, &gtk::graphene::Point::new(0.0, 0.0)) else { return };
                // the same cover fit as the wallpaper picture, seen through the card's place
                let scale = (ww / surface.width() as f64).max(wh / surface.height() as f64);
                cr.translate(-at.x() as f64 + (ww - surface.width() as f64 * scale) / 2.0, -at.y() as f64 + (wh - surface.height() as f64 * scale) / 2.0);
                cr.scale(scale, scale);
                let _ = cr.set_source_surface(surface, 0.0, 0.0);
                cr.source().set_filter(gtk::cairo::Filter::Good);
                let _ = cr.paint();
            }
        });
        card.set_child(Some(&frost));
        card.add_overlay(&body);
        card.set_measure_overlay(&body, true);

        // the card in the middle of the room left under the clock, nudged up for the bottom row
        let top_room = gtk::Box::builder().vexpand(true).build();
        let low_room = gtk::Box::builder().vexpand(true).margin_bottom(72).build();
        column.append(&top_room);
        column.append(&card);
        column.append(&low_room);

        let battery = battery(look);
        let form = Rc::new(Form {
            window: window.clone(),
            avatar,
            name,
            body,
            battery: battery.clone(),
            frost: frost.clone(),
            entry,
            go,
            field,
            note,
            layout,
            busy: Cell::new(false),
            armed: RefCell::default(),
        });
        overlay.add_overlay(&battery);
        overlay.add_overlay(&power_row(look, &form));
        built = Some(form);
    }
    overlay.add_overlay(&column);
    window.set_child(Some(&overlay));
    (window, time, date, built)
}

/// Take "waiting" off once the window's first frame has been drawn, which starts the entrance.
fn arrive(window: &gtk::Window) {
    let frames = Cell::new(0);
    window.add_tick_callback(move |window, _| {
        frames.set(frames.get() + 1);
        if frames.get() < 2 {
            return glib::ControlFlow::Continue;
        }
        window.remove_css_class("waiting");
        glib::ControlFlow::Break
    });
}

/// Run `then` once the short exit has played (straight away when animations are off).
fn after_exit(form: &Form, then: impl FnOnce() + 'static) {
    let motion = gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations());
    if !motion {
        then();
        return;
    }
    form.window.add_css_class("leaving");
    glib::timeout_add_local_once(std::time::Duration::from_millis(170), then);
}

/// Shake the password box once; the class comes off after, so the next wrong guess shakes again.
fn shake(form: &Rc<Form>) {
    form.field.remove_css_class("shake");
    form.field.add_css_class("shake");
    let field = form.field.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(460), move || field.remove_css_class("shake"));
}

fn say(form: &Form, text: &str, wrong: bool) {
    form.note.set_text(text);
    if wrong {
        form.note.add_css_class("wrong");
        form.field.add_css_class("wrong");
    } else {
        form.note.remove_css_class("wrong");
        form.field.remove_css_class("wrong");
    }
}

/// The usual line under the box: a Caps Lock warning when it is on, else nothing.
fn idle_note(form: &Form) {
    let caps = gdk::Display::default()
        .and_then(|d| d.default_seat())
        .and_then(|s| s.keyboard())
        .is_some_and(|k| k.is_caps_locked());
    say(form, if caps { "Caps Lock is on" } else { "" }, false);
}

fn wire(lock: &Lock, form: &Rc<Form>, person: &Rc<Person>) {
    let submit = {
        let (lock, form, person) = (lock.clone(), form.clone(), person.clone());
        move || {
            let password = form.entry.text().to_string();
            if form.busy.get() || password.is_empty() {
                return;
            }
            form.busy.set(true);
            form.field.add_css_class("busy");
            form.entry.set_sensitive(false);
            form.go.set_sensitive(false);
            say(&form, "Checking…", false);
            let login = person.login.clone();
            let check = gio::spawn_blocking(move || check(&login, &password));
            let (lock, form) = (lock.clone(), form.clone());
            glib::spawn_future_local(async move {
                let said = match check.await {
                    Ok(Ok(())) => {
                        say(&form, "", false);
                        after_exit(&form, move || lock.unlock());
                        return;
                    }
                    Ok(Err(said)) => said,
                    Err(_) => None,
                };
                form.busy.set(false);
                form.field.remove_css_class("busy");
                shake(&form);
                form.entry.set_text("");
                form.entry.set_sensitive(true);
                form.go.set_sensitive(true);
                form.entry.grab_focus();
                // PAM's own words when it has some (faillock: the account is locked, and for how long).
                say(&form, said.as_deref().unwrap_or("Wrong password. Try again."), true);
            });
        }
    };
    wire_form(form, Rc::new(submit));
}

/// What the password box does apart from checking the password: Enter and the arrow button run
/// `submit`, Esc clears, Caps Lock and the keyboard layout are shown, shortcuts typed out of habit
/// are swallowed. Shared by the lock screen and the sign-in screen.
fn wire_form(form: &Rc<Form>, submit: Rc<dyn Fn()>) {
    {
        let submit = submit.clone();
        form.entry.connect_activate(move |_| submit());
    }
    form.go.connect_clicked(move |_| submit());
    {
        let form_ref = form.clone();
        form.entry.connect_changed(move |_| {
            if !form_ref.busy.get() {
                idle_note(&form_ref);
            }
        });
    }
    // Esc clears what was typed.
    let keys = gtk::EventControllerKey::new();
    {
        let form = form.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape && !form.busy.get() {
                form.entry.set_text("");
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    form.entry.add_controller(keys);
    // A shortcut pressed out of habit (Super+B) must not type its letter into the password.
    let shortcuts = gtk::EventControllerKey::new();
    shortcuts.set_propagation_phase(gtk::PropagationPhase::Capture);
    shortcuts.connect_key_pressed(|_, _, _, modifiers| {
        if modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    form.entry.add_controller(shortcuts);
    {
        let field = form.field.clone();
        let focus = gtk::EventControllerFocus::new();
        let f2 = field.clone();
        focus.connect_enter(move |_| f2.add_css_class("focused"));
        focus.connect_leave(move |_| field.remove_css_class("focused"));
        form.entry.add_controller(focus);
    }
    if let Some(keyboard) = gdk::Display::default().and_then(|d| d.default_seat()).and_then(|s| s.keyboard()) {
        let form = form.clone();
        keyboard.connect_caps_lock_state_notify(move |_| {
            if !form.busy.get() {
                idle_note(&form);
            }
        });
    }
    if let Some(keyboard) = gdk::Display::default().and_then(|d| d.default_seat()).and_then(|s| s.keyboard()) {
        show_layout(form, &keyboard);
        let form = form.clone();
        keyboard.connect_active_layout_index_notify(move |keyboard| show_layout(&form, keyboard));
    }
    idle_note(form);
    form.entry.grab_focus();
}

/// The keyboard layout in use, shown only when there is more than one to switch between.
fn show_layout(form: &Form, keyboard: &gdk::Device) {
    let names = keyboard.layout_names();
    let current = usize::try_from(keyboard.active_layout_index()).ok().and_then(|i| names.get(i).map(|n| n.to_string()));
    match current {
        Some(name) if names.len() > 1 => {
            form.layout.set_text(&format!("Keyboard: {name}"));
            form.layout.set_visible(true);
        }
        _ => form.layout.set_visible(false),
    }
}

/// The battery, bottom left, when the computer has one: level and whether it is charging.
fn battery(look: &Look) -> gtk::Box {
    let row = gtk::Box::builder().spacing(8).css_classes(["edge"]).halign(gtk::Align::Start).valign(gtk::Align::End).margin_bottom(36).margin_start(32).visible(false).build();
    let icon_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let level = gtk::Label::builder().css_classes(["status"]).build();
    row.append(&icon_slot);
    row.append(&level);
    let icons = PathBuf::from(look.get("icons"));
    let update = {
        let (row, icon_slot, level) = (row.clone(), icon_slot.clone(), level.clone());
        move || {
            let Some((percent, charging)) = read_battery() else {
                row.set_visible(false);
                return;
            };
            let name = if charging { "battery-charging-bold" } else if percent <= 20 { "battery-low-bold" } else { "battery-high-bold" };
            while let Some(child) = icon_slot.first_child() {
                icon_slot.remove(&child);
            }
            let image = gtk::Image::from_file(icons.join(format!("{name}.svg")));
            image.set_pixel_size(18);
            icon_slot.append(&image);
            level.set_text(&format!("{percent}%"));
            row.update_property(&[gtk::accessible::Property::Label(&format!(
                "Battery {percent} percent{}",
                if charging { ", charging" } else { "" }
            ))]);
            row.set_visible(true);
        }
    };
    update();
    glib::timeout_add_seconds_local(30, move || {
        update();
        glib::ControlFlow::Continue
    });
    row
}

/// The first battery's level and whether it is charging, from /sys/class/power_supply.
fn read_battery() -> Option<(u32, bool)> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in dir.flatten() {
        let path = entry.path();
        let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() != "Battery" {
            continue;
        }
        let percent = std::fs::read_to_string(path.join("capacity")).ok()?.trim().parse().ok()?;
        let status = std::fs::read_to_string(path.join("status")).unwrap_or_default();
        return Some((percent, matches!(status.trim(), "Charging" | "Full")));
    }
    None
}

/// Sleep, Restart and Shut down, bottom right. Restart and Shut down ask for a second click,
/// since they end the session's open work.
fn power_row(look: &Look, form: &Rc<Form>) -> gtk::Box {
    let row = gtk::Box::builder().spacing(10).css_classes(["edge"]).halign(gtk::Align::End).valign(gtk::Align::End).margin_bottom(28).margin_end(28).build();
    for (icon, name, command, confirm) in [
        ("moon-bold", "Sleep", "suspend", false),
        ("arrow-clockwise-bold", "Restart", "reboot", true),
        ("power-bold", "Shut down", "poweroff", true),
    ] {
        // A click leaves the keyboard on the password box, so what is typed next is not lost and
        // Enter does not press this button again.
        let button = gtk::Button::builder().css_classes(["power"]).tooltip_text(name).focus_on_click(false).build();
        button.set_child(Some(&look.icon(icon, 18)));
        button.update_property(&[gtk::accessible::Property::Label(name)]);
        let form = form.clone();
        button.connect_clicked(move |button| {
            let ready = !confirm || form.armed.borrow().as_ref().is_some_and(|(armed, _)| armed == button);
            disarm(&form);
            form.entry.grab_focus();
            if ready {
                let _ = std::process::Command::new("systemctl").arg(command).spawn();
                return;
            }
            button.add_css_class("armed");
            say(&form, &format!("Click {name} again to {}.", name.to_lowercase()), false);
            let form_later = form.clone();
            let timer = glib::timeout_add_seconds_local_once(5, move || {
                form_later.armed.borrow_mut().take().map(|(b, _)| b.remove_css_class("armed"));
                idle_note(&form_later);
            });
            *form.armed.borrow_mut() = Some((button.clone(), timer));
        });
        row.append(&button);
    }
    row
}

/// Forgets a Restart or Shut down waiting for its second click.
fn disarm(form: &Form) {
    if let Some((button, timer)) = form.armed.borrow_mut().take() {
        button.remove_css_class("armed");
        timer.remove();
    }
}

/// The password check. Debug builds can stand in a test password so the unlock path can be tried
/// without a real one; release builds always ask PAM.
fn check(login: &str, password: &str) -> Result<(), Option<String>> {
    #[cfg(debug_assertions)]
    if let Ok(test) = std::env::var("ARKA_LOCK_TEST_PASSWORD") {
        std::thread::sleep(std::time::Duration::from_millis(600));
        return if password == test { Ok(()) } else { Err(None) };
    }
    pam::check(login, password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_desktop_look() {
        let values = parse("# from arka-desk\naccent = #A9C4FF\nwallpaper=/w/a b.jpg\n\nbad line\n");
        assert_eq!(values.get("accent").map(String::as_str), Some("#A9C4FF"));
        assert_eq!(values.get("wallpaper").map(String::as_str), Some("/w/a b.jpg"));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn colours_become_css() {
        let mut look = Look { values: HashMap::new() };
        look.values.insert("accent".into(), "#76CFFF".into());
        assert_eq!(look.rgba("accent", 0.3), "rgba(118, 207, 255, 0.3)");
    }

    #[test]
    fn the_badge_shows_the_first_letter() {
        assert_eq!(initial("ram"), "R");
        assert_eq!(initial(" Thulasi Ram"), "T");
        assert_eq!(initial(""), "?");
    }
}
