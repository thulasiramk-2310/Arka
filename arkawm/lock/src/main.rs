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
            .veil {{ background: linear-gradient(180deg, {bg_soft} 0%, {bg_mid} 55%, {bg_deep} 100%); }}
            .time {{ font-size: 104px; font-weight: 300; letter-spacing: -2px; font-feature-settings: "tnum"; }}
            .date {{ font-size: 19px; font-weight: 500; color: {dim}; }}
            .avatar {{ min-width: 76px; min-height: 76px; border-radius: 999px; background: {panel};
                       border: 1px solid {border}; color: {accent}; font-size: 32px; font-weight: 600; }}
            .name {{ font-size: 19px; font-weight: 600; }}
            .field {{ background: {panel}; border: 1px solid {border}; border-radius: 999px; padding: 4px 6px 4px 18px; }}
            .field.focused {{ border-color: {accent}; box-shadow: 0 0 0 3px {accent_ring}; }}
            .field.wrong {{ border-color: {warn}; }}
            .field entry {{ background: none; border: none; box-shadow: none; outline: none; color: {text};
                            font-size: 15px; min-height: 38px; padding: 0; caret-color: {accent}; }}
            .field entry > text > placeholder {{ color: {dim}; }}
            .go {{ min-width: 38px; min-height: 38px; border-radius: 999px; padding: 0; background: {accent};
                   color: {bg}; border: none; box-shadow: none; }}
            .go:hover {{ background: {accent_hover}; }}
            .go:disabled {{ background: {panel}; color: {dim}; }}
            .note {{ font-size: 13px; color: {dim}; min-height: 18px; }}
            .note.wrong {{ color: {warn}; }}
            .layout {{ font-size: 12px; color: {dim}; margin-top: 6px; }}
            .status {{ font-size: 13px; font-weight: 500; color: {text}; }}
            .power {{ min-width: 40px; min-height: 40px; border-radius: 999px; padding: 0; background: {panel};
                      border: 1px solid {border}; box-shadow: none; }}
            .power:hover {{ border-color: {accent}; }}
            .power:focus-visible {{ border-color: {accent}; box-shadow: 0 0 0 3px {accent_ring}; }}
            .power.armed {{ border-color: {warn}; }}
            "#,
            bg = self.get("background"),
            bg_soft = self.rgba("background", 0.35),
            bg_mid = self.rgba("background", 0.55),
            bg_deep = self.rgba("background", 0.8),
            font = self.get("font"),
            text = self.get("text"),
            dim = self.get("text_dim"),
            panel = self.rgba("panel", 0.82),
            border = self.rgba("border", 0.45),
            accent = self.get("accent"),
            accent_ring = self.rgba("accent", 0.3),
            accent_hover = self.rgba("accent", 0.85),
            warn = self.get("warn"),
        )
    }
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
        let (app, look, person, form, clocks) = (app.clone(), look.clone(), person.clone(), form.clone(), clocks.clone());
        lock.connect_monitor(move |lock, monitor| {
            let with_form = form.borrow().is_none();
            let (window, time, date, built) = screen(&app, &look, &person, with_form);
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

    if !lock.lock() {
        eprintln!("arka-lock: the screen could not be locked");
        failed.set(true);
        app.quit();
    }
}

/// One screen's window: wallpaper, veil, clock and date, and on the first screen the form.
/// A plain window: an application window crashed GTK when the lock library destroyed it at unlock.
fn screen(_app: &gtk::Application, look: &Look, person: &Person, with_form: bool) -> (gtk::Window, gtk::Label, gtk::Label, Option<Rc<Form>>) {
    let window = gtk::Window::builder().css_classes(["lock"]).build();
    let overlay = gtk::Overlay::new();

    let wallpaper = gtk::Picture::for_filename(look.get("wallpaper"));
    wallpaper.set_content_fit(gtk::ContentFit::Cover);
    wallpaper.set_can_shrink(true);
    overlay.set_child(Some(&wallpaper));
    let veil = gtk::Box::builder().css_classes(["veil"]).hexpand(true).vexpand(true).build();
    overlay.add_overlay(&veil);

    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .spacing(0)
        .build();
    let time = gtk::Label::builder().label(now("%H:%M")).css_classes(["time"]).build();
    let date = gtk::Label::builder().label(now("%A, %-d %B")).css_classes(["date"]).build();
    column.append(&time);
    column.append(&date);

    let mut built = None;
    if with_form {
        let who = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(64).halign(gtk::Align::Center).build();
        let avatar = gtk::Label::builder().label(initial(&person.shown)).css_classes(["avatar"]).halign(gtk::Align::Center).build();
        avatar.set_accessible_role(gtk::AccessibleRole::Presentation);
        let name = gtk::Label::builder().label(&person.shown).css_classes(["name"]).build();
        who.append(&avatar);
        who.append(&name);

        let entry = gtk::PasswordEntry::builder().show_peek_icon(true).hexpand(true).build();
        entry.set_placeholder_text(Some("Password"));
        entry.update_property(&[gtk::accessible::Property::Label("Password")]);
        let go = gtk::Button::builder().icon_name("go-next-symbolic").css_classes(["go"]).valign(gtk::Align::Center).build();
        go.set_tooltip_text(Some("Unlock"));
        go.update_property(&[gtk::accessible::Property::Label("Unlock")]);
        let field = gtk::Box::builder().spacing(8).css_classes(["field"]).width_request(340).margin_top(20).halign(gtk::Align::Center).build();
        field.append(&entry);
        field.append(&go);
        let note = gtk::Label::builder().css_classes(["note"]).margin_top(14).wrap(true).max_width_chars(48).justify(gtk::Justification::Center).build();
        note.set_accessible_role(gtk::AccessibleRole::Status);
        let layout = gtk::Label::builder().css_classes(["layout"]).visible(false).build();

        column.append(&who);
        column.append(&field);
        column.append(&note);
        column.append(&layout);
        let form = Rc::new(Form { entry, go, field, note, layout, busy: Cell::new(false), armed: RefCell::default() });

        overlay.add_overlay(&battery(look));
        overlay.add_overlay(&power_row(look, &form));
        built = Some(form);
    }
    overlay.add_overlay(&column);
    window.set_child(Some(&overlay));
    (window, time, date, built)
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
                        lock.unlock();
                        return;
                    }
                    Ok(Err(said)) => said,
                    Err(_) => None,
                };
                form.busy.set(false);
                form.entry.set_text("");
                form.entry.set_sensitive(true);
                form.go.set_sensitive(true);
                form.entry.grab_focus();
                // PAM's own words when it has some (faillock: the account is locked, and for how long).
                say(&form, said.as_deref().unwrap_or("Wrong password. Try again."), true);
            });
        }
    };
    let submit = Rc::new(submit);
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

/// The battery, top right, when the computer has one: level and whether it is charging.
fn battery(look: &Look) -> gtk::Box {
    let row = gtk::Box::builder().spacing(8).halign(gtk::Align::End).valign(gtk::Align::Start).margin_top(24).margin_end(28).visible(false).build();
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
    let row = gtk::Box::builder().spacing(10).halign(gtk::Align::End).valign(gtk::Align::End).margin_bottom(28).margin_end(28).build();
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
