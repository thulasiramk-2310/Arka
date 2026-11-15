//! What the top strip offers besides the brand and the clock: the row of open windows, and the
//! sheet that drops down on the right with notifications, quick settings or clipboard history.
//!
//! The window list and "show this sheet" requests come from ArkaWM over its socket
//! (`ARKAWM_SOCKET`). Notifications arrive over D-Bus (org.freedesktop.Notifications). Clipboard
//! history is kept in memory only and is gone at log-out.

use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;

use gtk::prelude::*;
use gtk::{gio, glib, Align, Orientation};
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::Ctx;

/// How many clipboard entries and notifications are kept.
const CLIPS: usize = 20;
const NOTES: usize = 50;
/// The clipboard watcher's process, to stop it when this program ends.
static CLIP_WATCHER: AtomicU32 = AtomicU32::new(0);

/// One open window, as ArkaWM reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: u32,
    pub focused: bool,
    pub minimized: bool,
    pub app_id: String,
    pub title: String,
}

/// The windows in a `windows …` line from ArkaWM (the part after the first word).
pub fn parse_windows(list: &str) -> Vec<Task> {
    list.split('\u{1f}')
        .filter_map(|record| {
            let mut fields = record.splitn(5, '\t');
            Some(Task {
                id: fields.next()?.parse().ok()?,
                focused: fields.next()? == "1",
                minimized: fields.next()? == "1",
                app_id: fields.next()?.to_string(),
                title: fields.next()?.to_string(),
            })
        })
        .collect()
}

/// What the quick settings could read. `None`: that setting is not available on this machine.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Quick {
    volume: Option<f64>,
    brightness: Option<f64>,
    wifi: Option<bool>,
    bluetooth: Option<bool>,
}

/// The percentage in `Volume: 0.45 [MUTED]`.
pub fn parse_volume(text: &str) -> Option<f64> {
    text.split_whitespace().nth(1)?.parse::<f64>().ok().map(|v| (v * 100.0).round())
}

/// The percentage in `amdgpu_bl1,backlight,120,47%,255`.
pub fn parse_brightness(text: &str) -> Option<f64> {
    text.lines().next()?.split(',').nth(3)?.trim_end_matches('%').parse().ok()
}

fn output_of(command: &str) -> Option<String> {
    // bounded: a hardware tool that hangs must not hang the desktop
    let out = Command::new("timeout").args(["2", "sh", "-c", command]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn read_quick() -> Quick {
    Quick {
        volume: output_of("wpctl get-volume @DEFAULT_AUDIO_SINK@").and_then(|t| parse_volume(&t)),
        brightness: output_of("brightnessctl -m").and_then(|t| parse_brightness(&t)),
        wifi: output_of("nmcli radio wifi").map(|t| t == "enabled"),
        bluetooth: output_of("rfkill -n -o SOFT list bluetooth").and_then(|t| match t.lines().next()? {
            "unblocked" => Some(true),
            "blocked" => Some(false),
            _ => None,
        }),
    }
}

fn spawn(command: &str) {
    let _ = Command::new("sh").arg("-c").arg(command).spawn();
}

enum Event {
    Windows(Vec<Task>),
    Show(String),
    Shot,
    ShotFailed,
    Clip(String),
    Quick(Quick),
}

thread_local! {
    /// Handles what the helper threads have sent; runs on the main thread.
    static DRAIN: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Called from a helper thread after it sent something.
fn wake() {
    glib::idle_add_once(|| DRAIN.with(|drain| drain.borrow().as_ref().map(|handle| handle())).unwrap_or(()));
}

pub struct Shell {
    ctx: Rc<Ctx>,
    socket: RefCell<Option<UnixStream>>,
    sender: mpsc::Sender<Event>,
    tasks: gtk::Box,
    bell: RefCell<Option<gtk::Button>>,
    panel: gtk::ApplicationWindow,
    stack: gtk::Stack,
    notes: gtk::Box,
    /// "Clear" over the notifications; shown only when there is something to clear.
    clear_notes: gtk::Button,
    clips: gtk::Box,
    history: RefCell<Vec<String>>,
    toast: gtk::ApplicationWindow,
    toast_title: gtk::Label,
    toast_body: gtk::Label,
    toast_turn: Cell<u32>,
    dnd: Cell<bool>,
    /// Whether any window is on screen (not minimised).
    windows_up: Cell<bool>,
    /// Set while a control is being moved to a value just read, so that does not count as the
    /// user changing it.
    applying: Cell<bool>,
    volume: gtk::Scale,
    brightness: gtk::Scale,
    wifi: gtk::Switch,
    bluetooth: gtk::Switch,
}

fn sheet_window(app: &gtk::Application, ctx: &Ctx, name: &str) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::new(app);
    window.add_css_class("sheetwin");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(name);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, ctx.px(8.0));
    window.set_margin(Edge::Right, ctx.px(16.0));
    window
}

fn clear(list: &gtk::Box) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn empty_note(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim");
    label.set_xalign(0.0);
    label
}

impl Shell {
    pub fn new(app: &gtk::Application, ctx: Rc<Ctx>) -> Rc<Self> {
        let (sender, receiver) = mpsc::channel();

        let tasks = gtk::Box::new(Orientation::Horizontal, ctx.px(8.0));
        tasks.set_valign(Align::Center);

        // The sheet.
        let panel = sheet_window(app, &ctx, "arka-panel");
        panel.set_keyboard_mode(KeyboardMode::OnDemand);
        let stack = gtk::Stack::new();
        stack.add_css_class("sheet");
        stack.set_size_request(ctx.px(360.0), -1);
        stack.set_vhomogeneous(false);
        panel.set_child(Some(&stack));

        let page = |name: &str| {
            let column = gtk::Box::new(Orientation::Vertical, ctx.px(12.0));
            stack.add_named(&column, Some(name));
            column
        };
        let head = |title: &str| {
            let row = gtk::Box::new(Orientation::Horizontal, ctx.px(8.0));
            let label = gtk::Label::new(Some(title));
            label.add_css_class("title");
            label.set_hexpand(true);
            label.set_xalign(0.0);
            row.append(&label);
            row
        };
        let link = |text: &str| {
            let button = gtk::Button::with_label(text);
            button.add_css_class("link");
            button
        };
        let scrolled = |list: &gtk::Box| {
            gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .max_content_height(ctx.px(420.0))
                .propagate_natural_height(true)
                .child(list)
                .build()
        };
        let setting = |icon: &str, name: &str, control: &gtk::Widget| {
            let row = gtk::Box::new(Orientation::Horizontal, ctx.px(12.0));
            row.add_css_class("row");
            row.append(&ctx.icon(icon, 20.0));
            let label = gtk::Label::new(Some(name));
            label.set_xalign(0.0);
            label.set_size_request(ctx.px(96.0), -1);
            row.append(&label);
            control.set_hexpand(true);
            control.set_halign(if control.is::<gtk::Switch>() { Align::End } else { Align::Fill });
            control.set_valign(Align::Center);
            row.append(control);
            row
        };

        // Notifications.
        let notes_page = page("notifications");
        let notes_head = head("Notifications");
        let clear_notes = link("Clear");
        clear_notes.set_visible(false);
        notes_head.append(&clear_notes);
        notes_page.append(&notes_head);
        let dnd = gtk::Switch::new();
        notes_page.append(&setting("bell-slash-bold", "Do Not Disturb", dnd.upcast_ref()));
        let notes = gtk::Box::new(Orientation::Vertical, ctx.px(8.0));
        notes.append(&empty_note("No notifications."));
        notes_page.append(&scrolled(&notes));

        // Quick settings.
        let quick_page = page("quick");
        quick_page.append(&head("Quick settings"));
        let volume = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
        let brightness = gtk::Scale::with_range(Orientation::Horizontal, 5.0, 100.0, 1.0);
        let wifi = gtk::Switch::new();
        let bluetooth = gtk::Switch::new();
        quick_page.append(&setting("speaker-high-bold", "Volume", volume.upcast_ref()));
        quick_page.append(&setting("sun-bold", "Brightness", brightness.upcast_ref()));
        quick_page.append(&setting("wifi-high-bold", "Wi-Fi", wifi.upcast_ref()));
        quick_page.append(&setting("bluetooth-bold", "Bluetooth", bluetooth.upcast_ref()));
        let themes = gtk::Box::new(Orientation::Horizontal, ctx.px(8.0));
        for (name, label) in crate::config::themes() {
            let choice = gtk::Button::with_label(&label);
            choice.add_css_class("choice");
            if name == ctx.cfg.theme {
                choice.add_css_class("on");
            } else {
                choice.connect_clicked(move |_| set_theme(&name));
            }
            themes.append(&choice);
        }
        quick_page.append(&setting("palette-bold", "Theme", themes.upcast_ref()));

        // Wallpaper: the pictures that come with the theme, and any picture of the user's own.
        let wall_head = gtk::Box::new(Orientation::Horizontal, ctx.px(12.0));
        wall_head.add_css_class("row");
        wall_head.append(&ctx.icon("image-bold", 20.0));
        let wall_label = gtk::Label::new(Some("Wallpaper"));
        wall_label.set_xalign(0.0);
        wall_head.append(&wall_label);
        quick_page.append(&wall_head);
        let walls = gtk::Box::new(Orientation::Horizontal, ctx.px(8.0));
        walls.set_margin_start(ctx.px(32.0));
        let current = Some(ctx.cfg.get("wallpaper.file")).filter(|f| !f.is_empty()).map(std::path::PathBuf::from);
        let pictures = ctx.cfg.wallpapers();
        let (thumb_w, thumb_h) = (ctx.px(56.0), ctx.px(32.0));
        for (i, path) in pictures.iter().enumerate() {
            let button = gtk::Button::new();
            button.add_css_class("thumb");
            button.set_overflow(gtk::Overflow::Hidden);
            button.set_size_request(thumb_w, thumb_h);
            button.set_hexpand(false);
            let shown = match &current {
                Some(file) => file == path,
                None => i == 0,
            };
            if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, thumb_w, thumb_h, false) {
                let picture = gtk::Picture::for_paintable(&gtk::gdk::Texture::for_pixbuf(&pixbuf));
                picture.set_content_fit(gtk::ContentFit::Cover);
                picture.set_can_shrink(true);
                picture.set_size_request(thumb_w, thumb_h);
                button.set_child(Some(&picture));
            }
            let name = if i == 0 { "The theme's picture".to_string() } else { picture_name(path) };
            button.set_tooltip_text(Some(&name));
            button.update_property(&[gtk::accessible::Property::Label(&format!("Wallpaper: {name}"))]);
            if shown {
                button.add_css_class("on");
            } else {
                // the theme's own picture is "no choice": it follows the theme
                let pick = if i == 0 { None } else { Some(path.clone()) };
                button.connect_clicked(move |_| set_wallpaper(pick.as_deref()));
            }
            walls.append(&button);
        }
        let own = gtk::Button::new();
        own.add_css_class("thumb");
        own.add_css_class("own");
        own.set_child(Some(&ctx.icon("folder-flat", 18.0)));
        own.set_size_request(thumb_h, thumb_h);
        own.set_tooltip_text(Some("A picture of your own"));
        own.update_property(&[gtk::accessible::Property::Label("Choose a picture of your own")]);
        if current.as_ref().is_some_and(|file| !pictures.contains(file)) {
            own.add_css_class("on");
        }
        own.connect_clicked({
            let panel = panel.clone();
            move |_| {
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Pictures"));
                for kind in ["image/jpeg", "image/png", "image/webp"] {
                    filter.add_mime_type(kind);
                }
                let filters = gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                let dialog = gtk::FileDialog::builder().title("Choose a wallpaper").modal(true).filters(&filters).build();
                let pictures_dir = glib::user_special_dir(glib::UserDirectory::Pictures);
                if let Some(dir) = pictures_dir {
                    dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
                }
                dialog.open(Some(&panel), gio::Cancellable::NONE, |result| {
                    if let Some(path) = result.ok().and_then(|file| file.path()) {
                        set_wallpaper(Some(&path));
                    }
                });
            }
        });
        walls.append(&own);
        quick_page.append(&walls);
        // The glowing paths in the rocks breathe slowly. Off by default: it costs a little power.
        let breathe = gtk::Switch::new();
        breathe.set_active(ctx.cfg.on("wallpaper.breathe"));
        breathe.set_tooltip_text(Some("The light paths in the rocks glow softly brighter and back. Uses a little more power."));
        breathe.connect_state_set(|_, on| {
            if let Some(dir) = settings_dir() {
                if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("breathe"), if on { "on\n" } else { "off\n" })).is_ok() {
                    start_again();
                }
            }
            glib::Propagation::Proceed
        });
        quick_page.append(&setting("sparkle-bold", "Glowing paths", breathe.upcast_ref()));

        // Clipboard history.
        let clips_page = page("clipboard");
        let clips_head = head("Clipboard");
        let clear_clips = link("Clear");
        clips_head.append(&clear_clips);
        clips_page.append(&clips_head);
        let clips = gtk::Box::new(Orientation::Vertical, ctx.px(8.0));
        clips.append(&empty_note("Nothing copied yet."));
        clips_page.append(&scrolled(&clips));
        clips_page.append(&empty_note("Kept in memory until you log out. Text a password manager marks as secret is skipped."));
        if let Some(note) = clips_page.last_child().and_downcast::<gtk::Label>() {
            note.set_wrap(true);
            note.set_max_width_chars(34);
        }

        // The toast: one short message under the strip.
        let toast = sheet_window(app, &ctx, "arka-toast");
        let toast_box = gtk::Box::new(Orientation::Vertical, ctx.px(4.0));
        toast_box.add_css_class("sheet");
        toast_box.set_size_request(ctx.px(320.0), -1);
        let toast_title = gtk::Label::new(None);
        toast_title.add_css_class("title");
        let toast_body = gtk::Label::new(None);
        toast_body.add_css_class("row");
        for label in [&toast_title, &toast_body] {
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_max_width_chars(34);
            toast_box.append(label);
        }
        toast.set_child(Some(&toast_box));

        let shell = Rc::new(Self {
            ctx,
            socket: RefCell::new(None),
            sender,
            tasks,
            bell: RefCell::new(None),
            panel,
            stack,
            notes,
            clear_notes: clear_notes.clone(),
            clips,
            history: RefCell::new(Vec::new()),
            toast,
            toast_title,
            toast_body,
            toast_turn: Cell::new(0),
            dnd: Cell::new(false),
            windows_up: Cell::new(false),
            applying: Cell::new(false),
            volume,
            brightness,
            wifi,
            bluetooth,
        });

        // Controls.
        let me = shell.clone();
        dnd.connect_state_set(move |_, on| {
            me.dnd.set(on);
            glib::Propagation::Proceed
        });
        let me = shell.clone();
        clear_notes.connect_clicked(move |_| {
            clear(&me.notes);
            me.notes.append(&empty_note("No notifications."));
            me.clear_notes.set_visible(false);
        });
        let me = shell.clone();
        clear_clips.connect_clicked(move |_| {
            me.history.borrow_mut().clear();
            me.show_clips();
        });
        let me = shell.clone();
        shell.volume.connect_value_changed(move |scale| {
            if !me.applying.get() {
                spawn(&format!("wpctl set-volume @DEFAULT_AUDIO_SINK@ {:.0}%", scale.value()));
            }
        });
        let me = shell.clone();
        shell.brightness.connect_value_changed(move |scale| {
            if !me.applying.get() {
                spawn(&format!("brightnessctl -q set {:.0}%", scale.value()));
            }
        });
        let me = shell.clone();
        shell.wifi.connect_state_set(move |_, on| {
            if !me.applying.get() {
                spawn(if on { "nmcli radio wifi on" } else { "nmcli radio wifi off" });
            }
            glib::Propagation::Proceed
        });
        let me = shell.clone();
        shell.bluetooth.connect_state_set(move |_, on| {
            if !me.applying.get() {
                spawn(if on { "rfkill unblock bluetooth" } else { "rfkill block bluetooth" });
            }
            glib::Propagation::Proceed
        });
        // Esc closes the sheet.
        let keys = gtk::EventControllerKey::new();
        let me = shell.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                me.panel.set_visible(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        shell.panel.add_controller(keys);

        let me = shell.clone();
        DRAIN.with(|drain| {
            *drain.borrow_mut() = Some(Box::new(move || {
                while let Ok(event) = receiver.try_recv() {
                    me.handle(event);
                }
            }));
        });
        shell
    }

    /// The row of open windows, for the middle of the strip.
    pub fn tasks(&self) -> &gtk::Box {
        &self.tasks
    }

    /// The bell button, so it can show that something new arrived.
    pub fn set_bell(&self, bell: &gtk::Button) {
        *self.bell.borrow_mut() = Some(bell.clone());
    }

    /// Connect to ArkaWM and start watching the clipboard.
    pub fn start(self: &Rc<Self>) {
        if let Some(mut stream) = std::env::var_os("ARKAWM_SOCKET").and_then(|path| UnixStream::connect(path).ok()) {
            if stream.write_all(b"watch\n").is_ok() {
                if let Ok(reader) = stream.try_clone() {
                    let sender = self.sender.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(reader).lines().map_while(Result::ok) {
                            let (word, rest) = line.split_once(' ').unwrap_or((&line, ""));
                            let event = match word {
                                "windows" => Event::Windows(parse_windows(rest)),
                                "show" => Event::Show(rest.to_string()),
                                "shot" => Event::Shot,
                                "shot-failed" => Event::ShotFailed,
                                _ => continue,
                            };
                            if sender.send(event).is_err() {
                                break;
                            }
                            wake();
                        }
                    });
                }
                *self.socket.borrow_mut() = Some(stream);
            }
        }

        // Each time something is copied, wl-paste runs the small script, which hands the text
        // over as one base64 line. Text marked as a secret by a password manager is left out.
        let script = "wl-paste -l 2>/dev/null | grep -q passwordManagerHint && exit 0; head -c 8000 | base64 -w0; echo";
        let watcher = Command::new("wl-paste")
            .args(["--type", "text", "--watch", "sh", "-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = watcher {
            CLIP_WATCHER.store(child.id(), Ordering::Relaxed);
            if let Some(out) = child.stdout.take() {
                let sender = self.sender.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(out).lines().map_while(Result::ok) {
                        let text = String::from_utf8_lossy(&glib::base64_decode(line.trim())).into_owned();
                        if sender.send(Event::Clip(text)).is_err() {
                            break;
                        }
                        wake();
                    }
                    let _ = child.wait();
                });
            }
        }
    }

    /// Back to the bare desktop: shut the sheet, and put away the windows if any are up.
    /// With none up this asks ArkaWM for nothing, so a second click does not bring them back.
    pub fn show_desktop(&self) {
        self.panel.set_visible(false);
        if self.windows_up.get() {
            self.send("show-desktop\n");
        }
    }

    fn send(&self, line: &str) {
        if let Some(socket) = self.socket.borrow_mut().as_mut() {
            let _ = socket.write_all(line.as_bytes());
        }
    }

    fn handle(self: &Rc<Self>, event: Event) {
        match event {
            Event::Windows(tasks) => self.show_tasks(&tasks),
            Event::Show(name) => self.toggle(&name),
            Event::Shot => self.toast("Screenshot saved", "It is in Pictures, in the Screenshots folder."),
            Event::ShotFailed => self.toast("Screenshot not saved", "The Pictures folder could not be written to."),
            Event::Clip(text) => {
                if text.trim().is_empty() {
                    return;
                }
                let mut history = self.history.borrow_mut();
                history.retain(|old| *old != text);
                history.insert(0, text);
                history.truncate(CLIPS);
                drop(history);
                self.show_clips();
            }
            Event::Quick(quick) => {
                self.applying.set(true);
                for (scale, value) in [(&self.volume, quick.volume), (&self.brightness, quick.brightness)] {
                    scale.set_sensitive(value.is_some());
                    if let Some(value) = value {
                        scale.set_value(value);
                    }
                }
                for (switch, value) in [(&self.wifi, quick.wifi), (&self.bluetooth, quick.bluetooth)] {
                    switch.set_sensitive(value.is_some());
                    switch.set_active(value.unwrap_or(false));
                }
                self.applying.set(false);
            }
        }
    }

    /// Open the sheet on this page, or close it if it is already showing that page.
    pub fn toggle(self: &Rc<Self>, name: &str) {
        let showing = self.panel.is_visible() && self.stack.visible_child_name().as_deref() == Some(name);
        if showing || self.stack.child_by_name(name).is_none() {
            self.panel.set_visible(false);
            return;
        }
        self.stack.set_visible_child_name(name);
        self.toast.set_visible(false);
        self.panel.present();
        match name {
            "quick" => {
                // read on another thread: these tools talk to hardware
                let sender = self.sender.clone();
                std::thread::spawn(move || {
                    if sender.send(Event::Quick(read_quick())).is_ok() {
                        wake();
                    }
                });
            }
            "notifications" => {
                if let Some(bell) = self.bell.borrow().as_ref() {
                    bell.remove_css_class("new");
                }
            }
            _ => {}
        }
    }

    fn toast(self: &Rc<Self>, title: &str, body: &str) {
        self.toast_title.set_text(title);
        self.toast_body.set_text(body);
        self.toast_body.set_visible(!body.is_empty());
        self.toast.present();
        let turn = self.toast_turn.get() + 1;
        self.toast_turn.set(turn);
        let me = self.clone();
        glib::timeout_add_seconds_local_once(5, move || {
            // a newer toast keeps the window up
            if me.toast_turn.get() == turn {
                me.toast.set_visible(false);
            }
        });
    }

    /// A notification arrived.
    fn notify(self: &Rc<Self>, app: &str, summary: &str, body: &str) {
        if self.notes.first_child().is_some_and(|child| child.is::<gtk::Label>()) {
            clear(&self.notes);
        }
        let item = gtk::Box::new(Orientation::Vertical, self.ctx.px(2.0));
        item.add_css_class("item");
        let time = glib::DateTime::now_local().ok().and_then(|now| now.format("%H:%M").ok()).unwrap_or_default();
        let from = if app.is_empty() { time.to_string() } else { format!("{app}  ·  {time}") };
        for (text, class) in [(from.as_str(), "dim"), (summary, "title"), (body, "row")] {
            if text.is_empty() {
                continue;
            }
            let label = gtk::Label::new(Some(text));
            label.add_css_class(class);
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_max_width_chars(34);
            item.append(&label);
        }
        self.notes.prepend(&item);
        self.clear_notes.set_visible(true);
        let mut count = 0;
        let mut child = self.notes.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            count += 1;
            if count > NOTES {
                self.notes.remove(&widget);
            }
        }
        let reading = self.panel.is_visible() && self.stack.visible_child_name().as_deref() == Some("notifications");
        if reading {
            return;
        }
        if let Some(bell) = self.bell.borrow().as_ref() {
            bell.add_css_class("new");
        }
        if !self.dnd.get() {
            self.toast(summary, body);
        }
    }

    fn show_clips(self: &Rc<Self>) {
        clear(&self.clips);
        let history = self.history.borrow();
        if history.is_empty() {
            self.clips.append(&empty_note("Nothing copied yet."));
        }
        for text in history.iter() {
            let first = text.trim().lines().next().unwrap_or_default();
            let label = gtk::Label::new(Some(first));
            label.add_css_class("row");
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(34);
            let button = gtk::Button::new();
            button.set_child(Some(&label));
            button.add_css_class("item");
            button.set_tooltip_text(Some("Copy this again"));
            let (me, text) = (self.clone(), text.clone());
            button.connect_clicked(move |_| {
                if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                }
                me.panel.set_visible(false);
            });
            self.clips.append(&button);
        }
    }

    fn show_tasks(self: &Rc<Self>, tasks: &[Task]) {
        self.windows_up.set(tasks.iter().any(|task| !task.minimized));
        clear(&self.tasks);
        let theme = gtk::gdk::Display::default().map(|display| gtk::IconTheme::for_display(&display));
        for task in tasks {
            let row = gtk::Box::new(Orientation::Horizontal, self.ctx.px(8.0));
            let names = [task.app_id.clone(), task.app_id.to_lowercase(), task.app_id.rsplit('.').next().unwrap_or_default().to_lowercase()];
            let known = names.iter().find(|name| !name.is_empty() && theme.as_ref().is_some_and(|t| t.has_icon(name)));
            let image = match known {
                Some(name) => {
                    let image = gtk::Image::from_icon_name(name);
                    image.set_pixel_size(self.ctx.px(18.0));
                    image
                }
                None => self.ctx.icon("squares-four-fill", 16.0),
            };
            row.append(&image);
            let label = gtk::Label::new(Some(if task.title.is_empty() { "Untitled window" } else { &task.title }));
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(16);
            row.append(&label);
            let button = gtk::Button::new();
            button.set_child(Some(&row));
            button.add_css_class("task");
            if task.focused {
                button.add_css_class("focused");
            }
            if task.minimized {
                button.add_css_class("minimized");
            }
            button.set_tooltip_text(Some(&task.title));
            let (me, id) = (self.clone(), task.id);
            button.connect_clicked(move |_| me.send(&format!("toggle {id}\n")));
            // middle click closes, as on a taskbar
            let middle = gtk::GestureClick::new();
            middle.set_button(2);
            let me = self.clone();
            middle.connect_pressed(move |_, _, _, _| me.send(&format!("close {id}\n")));
            button.add_controller(middle);
            self.tasks.append(&button);
        }
    }
}

/// Stop the clipboard watcher; it would otherwise outlive this program.
fn stop_watcher() {
    let pid = CLIP_WATCHER.swap(0, Ordering::Relaxed);
    if pid != 0 {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}

fn settings_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".config/arkawm"))
}

/// "lake-sunrise.jpg" -> "Lake sunrise"
fn picture_name(path: &std::path::Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().replace(['-', '_'], " ");
    let mut chars = stem.chars();
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// Remember the wallpaper (None: the theme's own) and start again with it.
fn set_wallpaper(path: Option<&std::path::Path>) {
    let Some(dir) = settings_dir() else { return };
    let file = dir.join("wallpaper");
    let saved = match path {
        Some(path) => std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, format!("{}\n", path.display()))),
        None => std::fs::remove_file(&file).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
    };
    if saved.is_ok() {
        start_again();
    }
}

/// Remember the theme and start again with it. A theme brings its own picture, so a picture
/// picked before is let go.
fn set_theme(name: &str) {
    let Some(dir) = settings_dir() else { return };
    if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("theme"), format!("{name}\n"))).is_err() {
        return;
    }
    let _ = std::fs::remove_file(dir.join("wallpaper"));
    start_again();
}

/// Start a fresh copy of the desktop with the settings just saved, and leave.
fn start_again() {
    stop_watcher();
    if let Ok(me) = std::env::current_exe() {
        // the choice in the file wins over a theme forced for testing
        if Command::new(me).env_remove("ARKA_THEME").spawn().is_ok() {
            std::process::exit(0);
        }
    }
}

const NOTIFICATIONS: &str = r#"<node><interface name="org.freedesktop.Notifications">
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="GetServerInformation"><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/></method>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>"#;

/// Become the desktop's notification service. If another one already runs (inside another
/// desktop, say), that one keeps the job and nothing happens here.
pub fn serve_notifications(shell: &Rc<Shell>) {
    let Some(interface) = gio::DBusNodeInfo::for_xml(NOTIFICATIONS).ok().and_then(|node| node.lookup_interface("org.freedesktop.Notifications"))
    else {
        return;
    };
    let shell = shell.clone();
    let next_id = Rc::new(Cell::new(1u32));
    gio::bus_own_name(
        gio::BusType::Session,
        "org.freedesktop.Notifications",
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            let (shell, next_id) = (shell.clone(), next_id.clone());
            let registered = connection
                .register_object("/org/freedesktop/Notifications", &interface)
                .method_call(move |_, _, _, _, method, params, invocation| match method {
                    "Notify" => {
                        let text = |index: usize| params.child_value(index).get::<String>().unwrap_or_default();
                        let replaces = params.child_value(1).get::<u32>().unwrap_or(0);
                        let id = if replaces != 0 { replaces } else { next_id.replace(next_id.get().wrapping_add(1).max(1)) };
                        shell.notify(&text(0), &text(3), &text(4));
                        invocation.return_value(Some(&(id,).to_variant()));
                    }
                    "GetCapabilities" => invocation.return_value(Some(&(vec!["body".to_string()],).to_variant())),
                    "GetServerInformation" => {
                        invocation.return_value(Some(&("Arka Desk", "ArkaOS", env!("CARGO_PKG_VERSION"), "1.2").to_variant()))
                    }
                    _ => invocation.return_value(None),
                })
                .build();
            if let Err(err) = registered {
                eprintln!("arka-desk: notifications not served: {err}");
            }
        },
        |_, _| {},
        |_, _| {},
    );
}

/// Stop helpers when the program ends normally.
pub fn shutdown() {
    stop_watcher();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_list_is_read() {
        let tasks = parse_windows("3\t1\t0\torg.kde.kate\tWelcome — Kate\u{1f}4\t0\t1\t\ta\tb");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], Task { id: 3, focused: true, minimized: false, app_id: "org.kde.kate".into(), title: "Welcome — Kate".into() });
        // a tab inside the last field stays in the title
        assert_eq!((tasks[1].minimized, tasks[1].app_id.as_str(), tasks[1].title.as_str()), (true, "", "a\tb"));
        assert!(parse_windows("").is_empty());
        assert!(parse_windows("x\t1\t0\ta\tb").is_empty());
    }

    #[test]
    fn tool_output_is_read() {
        assert_eq!(parse_volume("Volume: 0.45"), Some(45.0));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]"), Some(100.0));
        assert_eq!(parse_volume(""), None);
        assert_eq!(parse_brightness("amdgpu_bl1,backlight,120,47%,255"), Some(47.0));
        assert_eq!(parse_brightness("nothing"), None);
    }
}
