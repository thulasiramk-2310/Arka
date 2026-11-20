//! Arka Install: the screens that install ArkaOS from the live system, next to Windows, on an
//! empty disk, or over a whole disk. They run unprivileged and leave every disk decision to the
//! backend (`arka-install`, see backend.rs): it reads the disks, plans the change with
//! systemd-repart's dry run, and writes only after the review.
//!
//! Pages: welcome (keyboard, before you start) → where (one card per possible install) →
//! encryption → review (the backend's plan, line by line) → installing → done / failed.
//!
//! Previews for design review: ARKA_INSTALL_PREVIEW=<page> opens a window that walks itself to
//! that page (with ARKA_INSTALL_BACKEND pointing at a fake backend), ARKA_INSTALL_SIZE=WxH sets
//! the window, ARKA_INSTALL_THEME=light|dark the look, ARKA_SHOT=<png> saves a picture.

mod backend;
mod look;
#[path = "../../arka-oobe/src/motion.rs"]
#[allow(dead_code)]
mod motion;
#[path = "../../arka-oobe/src/ridge.rs"]
#[allow(dead_code)]
mod ridge;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use backend::{Choice, Disk, Event, Outcome, Plan, Said};
use gtk4::{gio, glib};
use libadwaita as adw;
use look::sp;

const APP_ID: &str = "org.arka.install";
const PLAN_FILE: &str = "/run/arka-install/plan.json";
const PAGES: &[&str] = &["welcome", "where", "encrypt", "review", "installing", "done", "failed"];
/// Ridge peak for each page (the setup's ridge has nine).
const STOPS: &[usize] = &[0, 2, 4, 6, 7, 8, 7];
/// The backend's steps, in order, with what the person reads for each.
const STEPS: &[(&str, &str)] = &[
    ("check", "Checking the disk again"),
    ("partition", "Creating the ArkaOS partitions"),
    ("keys", "Setting up the disk keys"),
    ("install", "Copying ArkaOS onto the disk"),
    ("verify", "Checking the result"),
    ("boot-menu", "Adding the start-up entries"),
];

fn main() {
    let mut flags = gio::ApplicationFlags::empty();
    if std::env::var_os("ARKA_INSTALL_PREVIEW").is_some() {
        // Several previews can render at once.
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = adw::Application::builder().application_id(APP_ID).flags(flags).build();
    app.connect_activate(|app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        App::build(app);
    });
    std::process::exit(app.run().value());
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.add_css_class(class);
    l.set_xalign(0.0);
    l.set_halign(gtk4::Align::Fill);
    l.set_wrap(true);
    l.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    // Wrapped labels otherwise ask for a narrow natural width and break lines early.
    l.set_max_width_chars(90);
    l
}

fn clear(b: &gtk4::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// A page: title, subtitle, body, in a centred column at most 680px wide that scrolls when
/// the screen is short. Same shape as the setup's pages (motion.rs looks for `oobe-page`).
fn page(title: &str, sub: &gtk4::Label, body: &impl IsA<gtk4::Widget>) -> gtk4::ScrolledWindow {
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    col.add_css_class("oobe-page");
    sub.set_margin_top(sp(14));
    body.as_ref().set_margin_top(sp(36));
    col.append(&label(title, "oobe-title"));
    col.append(sub);
    col.append(body);
    let clamp = adw::Clamp::builder()
        .maximum_size(sp(680))
        .tightening_threshold(sp(560))
        .child(&col)
        .valign(gtk4::Align::Center)
        .margin_start(sp(24))
        .margin_end(sp(24))
        .margin_top(sp(24))
        .margin_bottom(sp(24))
        .build();
    gtk4::ScrolledWindow::builder().hscrollbar_policy(gtk4::PolicyType::Never).child(&clamp).build()
}

/// A line with a leading dot, for the review.
fn dot_line(text: &str, kind: &str) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, sp(14));
    let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    dot.add_css_class("ai-dot");
    if !kind.is_empty() {
        dot.add_css_class(kind);
    }
    dot.set_valign(gtk4::Align::Start);
    dot.set_margin_top(sp(8));
    row.append(&dot);
    let l = label(text, "ai-line");
    l.set_hexpand(true);
    row.append(&l);
    row
}

fn warnbox(lines: &[String]) -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, sp(8));
    b.add_css_class("ai-warnbox");
    for w in lines {
        b.append(&label(w, "ai-line"));
    }
    b
}

fn busy(text: &str) -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, sp(12));
    let s = gtk4::Spinner::new();
    s.start();
    b.append(&s);
    b.append(&label(text, "oobe-sub"));
    b
}

/// One line on the installing page: a mark (waiting ring, spinner, done dot), the step, and
/// how long it has taken.
struct StepRow {
    mark: gtk4::Stack,
    spinner: gtk4::Spinner,
    text: gtk4::Label,
    time: gtk4::Label,
}

fn mmss(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

#[derive(Default)]
struct St {
    keymap: String,
    choices: Vec<Choice>,
    /// Disk names for the erase confirmation, by path.
    disks: Vec<Disk>,
    picked: Option<usize>,
    encrypt: bool,
    /// This computer has a TPM 2.0 chip.
    tpm_chip: bool,
    /// Unlock with the chip and a PIN (else a passphrase).
    use_tpm: bool,
    /// The last page shows a recovery key and the person said it is saved.
    key_shown: bool,
    key_saved: bool,
    plan: Option<Plan>,
    last_step: Option<String>,
    installing: bool,
    /// The step on screen now and when it started.
    step_start: Option<(usize, std::time::Instant)>,
    log: Vec<String>,
}

struct App {
    window: adw::ApplicationWindow,
    pages: gtk4::Stack,
    ridge: Rc<ridge::Ridge>,
    step_lbl: gtk4::Label,
    back: gtk4::Button,
    next: gtk4::Button,
    err: gtk4::Label,
    cur: Cell<usize>,
    st: RefCell<St>,
    image: String,
    preview: Option<String>,
    // where
    where_body: gtk4::Box,
    // encryption
    pass: gtk4::PasswordEntry,
    again: gtk4::PasswordEntry,
    no_encrypt: gtk4::CheckButton,
    use_pass: gtk4::CheckButton,
    pass_box: gtk4::Box,
    enc_note: gtk4::Label,
    enc_sub: gtk4::Label,
    // review
    review_body: gtk4::Box,
    // installing
    step_rows: Vec<StepRow>,
    log_lbl: gtk4::Label,
    // done / failed
    done_body: gtk4::Box,
    fail_sub: gtk4::Label,
    fail_body: gtk4::Box,
}

impl App {
    fn build(app: &adw::Application) {
        let preview = std::env::var("ARKA_INSTALL_PREVIEW").ok();
        let size = std::env::var("ARKA_INSTALL_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<i32>().ok()?, h.parse::<i32>().ok()?))
        });
        let screen = size.or_else(|| {
            let monitors = gtk4::gdk::Display::default()?.monitors();
            let m = monitors.item(0)?.downcast::<gtk4::gdk::Monitor>().ok()?;
            Some((m.geometry().width(), m.geometry().height()))
        });
        if let Some((w, h)) = screen {
            look::SCALE.with(|s| s.set(look::scale_for(w, h)));
        }

        // Light unless the session asks for dark (or a preview forces one).
        let manager = adw::StyleManager::default();
        match std::env::var("ARKA_INSTALL_THEME").as_deref() {
            Ok("dark") => manager.set_color_scheme(adw::ColorScheme::ForceDark),
            Ok("light") => manager.set_color_scheme(adw::ColorScheme::ForceLight),
            _ => manager.set_color_scheme(adw::ColorScheme::PreferLight),
        }
        let dark = Rc::new(Cell::new(manager.is_dark()));
        let provider = gtk4::CssProvider::new();
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
        provider.load_from_string(&look::scale_css(&look::css(dark.get()), look::SCALE.with(Cell::get)));

        let window = adw::ApplicationWindow::builder().application(app).title("Install ArkaOS").default_width(1280).default_height(800).build();
        if let Some((w, h)) = size {
            window.set_default_size(w, h);
        }
        if preview.is_none() {
            window.fullscreen();
        }

        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.add_css_class("oobe-root");
        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, sp(10));
        top.set_margin_top(sp(36));
        top.set_margin_start(sp(44));
        top.set_margin_end(sp(44));
        if let Ok(tex) = gtk4::gdk::Texture::from_bytes(&glib::Bytes::from_static(include_bytes!("../../arka-oobe/src/logo.png"))) {
            let img = gtk4::Image::from_paintable(Some(&tex));
            img.set_pixel_size(sp(34));
            top.append(&img);
        }
        top.append(&label("Install ArkaOS", "oobe-brand"));
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        top.append(&spacer);
        let step_lbl = label("", "oobe-step");
        top.append(&step_lbl);

        let pages = gtk4::Stack::new();
        pages.set_transition_type(gtk4::StackTransitionType::None);
        pages.set_vexpand(true);

        // ── welcome ──
        let keymaps = backend::keymaps();
        let shown: Vec<String> = keymaps.iter().map(|k| backend::keymap_name(k)).collect();
        let refs: Vec<&str> = shown.iter().map(String::as_str).collect();
        let keymap = gtk4::DropDown::from_strings(&refs);
        keymap.add_css_class("oobe-field");
        keymap.set_enable_search(true);
        keymap.set_expression(Some(gtk4::PropertyExpression::new(gtk4::StringObject::static_type(), None::<gtk4::Expression>, "string")));
        let us = keymaps.iter().position(|k| k == "us").unwrap_or(0);
        keymap.set_selected(us as u32);
        let wbody = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
        wbody.append(&label("Keyboard layout", "ai-section"));
        wbody.append(&keymap);
        wbody.append(&label("The layout you will type the disk PIN or passphrase with when the computer starts.", "oobe-note"));
        let before = gtk4::Box::new(gtk4::Orientation::Vertical, sp(12));
        before.set_margin_top(sp(36));
        before.append(&label("Before you start", "ai-section"));
        before.append(&dot_line("Back up the files you can't lose. Installing changes a disk.", ""));
        before.append(&dot_line(
            "If Windows uses BitLocker, save its recovery key first. Windows may ask for it once after ArkaOS is installed.",
            "",
        ));
        before.append(&dot_line(
            "Installing next to Windows is new in this Developer Preview. It has been tested in virtual machines and on one laptop so far.",
            "",
        ));
        if backend::on_battery() {
            before.append(&dot_line("This computer is running on battery. Connect the charger before installing.", "erase"));
        }
        wbody.append(&before);
        let wsub = label("Nothing is written to any disk until you confirm on the review page.", "oobe-sub");
        pages.add_named(&page("Before you install", &wsub, &wbody), Some("welcome"));

        // ── where ──
        let where_body = gtk4::Box::new(gtk4::Orientation::Vertical, sp(12));
        let where_sub = label("Pick one. You can still go back after this.", "oobe-sub");
        pages.add_named(&page("Where to install", &where_sub, &where_body), Some("where"));

        // ── encryption ──
        let ebody = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
        let pass_box = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
        let pass = gtk4::PasswordEntry::new();
        pass.set_show_peek_icon(true);
        pass.set_property("placeholder-text", "Passphrase");
        pass.add_css_class("oobe-field");
        let again = gtk4::PasswordEntry::new();
        again.set_show_peek_icon(true);
        again.set_property("placeholder-text", "Type it again");
        again.add_css_class("oobe-field");
        pass_box.append(&pass);
        pass_box.append(&again);
        let enc_note = label("", "oobe-note");
        pass_box.append(&enc_note);
        ebody.append(&pass_box);
        let tpm_chip = backend::has_tpm2();
        let use_pass = gtk4::CheckButton::with_label("Use a long passphrase instead of the TPM chip");
        use_pass.add_css_class("oobe-tool");
        use_pass.set_margin_top(sp(24));
        use_pass.set_visible(tpm_chip);
        ebody.append(&use_pass);
        let no_encrypt = gtk4::CheckButton::with_label("Don't encrypt ArkaOS");
        no_encrypt.add_css_class("oobe-tool");
        no_encrypt.set_margin_top(sp(if tpm_chip { 4 } else { 24 }));
        ebody.append(&no_encrypt);
        let esub = label("", "oobe-sub");
        pages.add_named(&page(if tpm_chip { "Disk encryption" } else { "Choose a disk passphrase" }, &esub, &ebody), Some("encrypt"));

        // ── review ──
        let review_body = gtk4::Box::new(gtk4::Orientation::Vertical, sp(14));
        let rsub = label("Nothing has been written yet. This is the plan the installer will carry out.", "oobe-sub");
        pages.add_named(&page("Review", &rsub, &review_body), Some("review"));

        // ── installing ──
        let ibody = gtk4::Box::new(gtk4::Orientation::Vertical, sp(16));
        let mut step_rows = vec![];
        for (_, text) in STEPS {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, sp(14));
            let mark = gtk4::Stack::new();
            mark.set_size_request(sp(18), sp(18));
            mark.set_valign(gtk4::Align::Center);
            for (name, kind) in [("wait", "wait"), ("done", "create")] {
                let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                dot.add_css_class("ai-dot");
                dot.add_css_class(kind);
                dot.set_halign(gtk4::Align::Center);
                dot.set_valign(gtk4::Align::Center);
                mark.add_named(&dot, Some(name));
            }
            let spinner = gtk4::Spinner::new();
            mark.add_named(&spinner, Some("now"));
            mark.set_visible_child_name("wait");
            let text = label(text, "oobe-tstep");
            text.set_hexpand(true);
            let time = label("", "ai-time");
            time.set_hexpand(false);
            row.append(&mark);
            row.append(&text);
            row.append(&time);
            ibody.append(&row);
            step_rows.push(StepRow { mark, spinner, text, time });
        }
        let details = gtk4::Expander::new(Some("Details"));
        details.add_css_class("ai-adv");
        details.set_margin_top(sp(20));
        let log_lbl = label("", "ai-log");
        log_lbl.set_selectable(true);
        details.set_child(Some(&log_lbl));
        ibody.append(&details);
        let isub = label("This takes a few minutes. Keep the computer on until it finishes.", "oobe-sub");
        pages.add_named(&page("Installing ArkaOS", &isub, &ibody), Some("installing"));

        // ── done / failed ──
        let done_body = gtk4::Box::new(gtk4::Orientation::Vertical, sp(14));
        // The installer runs from the USB stick: taking it out first leaves nothing to restart with.
        let dsub = label("Click Restart. Take out the USB stick or disc when the screen goes dark.", "oobe-sub");
        pages.add_named(&page("ArkaOS is installed", &dsub, &done_body), Some("done"));
        let fail_body = gtk4::Box::new(gtk4::Orientation::Vertical, sp(14));
        let fail_sub = label("", "oobe-sub");
        pages.add_named(&page("The install stopped", &fail_sub, &fail_body), Some("failed"));

        // ── bottom bar ──
        let err = label("", "oobe-err");
        err.set_visible(false);
        err.set_valign(gtk4::Align::Center);
        let back = gtk4::Button::with_label("Back");
        back.add_css_class("oobe-back");
        let next = gtk4::Button::with_label("Continue");
        next.add_css_class("oobe-primary");
        let bottom = gtk4::Box::new(gtk4::Orientation::Horizontal, sp(12));
        bottom.set_margin_bottom(sp(6));
        bottom.set_margin_start(sp(44));
        bottom.set_margin_end(sp(44));
        let bspace = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        bspace.set_hexpand(true);
        bottom.append(&err);
        bottom.append(&bspace);
        bottom.append(&back);
        bottom.append(&next);

        let ridge = ridge::Ridge::new(dark.clone());
        frame.append(&top);
        frame.append(&pages);
        frame.append(&bottom);
        frame.append(&ridge.area);
        window.set_content(Some(&frame));

        let image = std::env::var("ARKA_INSTALL_IMAGE").unwrap_or_else(|_| backend::default_image());
        let a = Rc::new(App {
            window: window.clone(),
            pages,
            ridge,
            step_lbl,
            back: back.clone(),
            next: next.clone(),
            err,
            cur: Cell::new(0),
            st: RefCell::new(St { keymap: keymaps[us].clone(), encrypt: true, tpm_chip, use_tpm: tpm_chip, ..Default::default() }),
            image,
            preview,
            where_body,
            pass: pass.clone(),
            again: again.clone(),
            no_encrypt: no_encrypt.clone(),
            use_pass: use_pass.clone(),
            pass_box,
            enc_note,
            enc_sub: esub,
            review_body,
            step_rows,
            log_lbl,
            done_body,
            fail_sub,
            fail_body,
        });

        {
            let (a2, provider, dark) = (a.clone(), provider.clone(), dark.clone());
            manager.connect_dark_notify(move |m| {
                dark.set(m.is_dark());
                provider.load_from_string(&look::scale_css(&look::css(m.is_dark()), look::SCALE.with(Cell::get)));
                a2.ridge.area.queue_draw();
            });
        }
        {
            let a = a.clone();
            keymap.connect_selected_notify(move |d| {
                if let Some(k) = keymaps.get(d.selected() as usize) {
                    a.st.borrow_mut().keymap = k.clone();
                }
            });
        }
        for e in [&pass, &again] {
            let a1 = a.clone();
            e.connect_changed(move |_| a1.refresh());
            let a = a.clone();
            e.connect_activate(move |_| {
                if a.next.is_sensitive() {
                    a.forward();
                }
            });
        }
        {
            let a = a.clone();
            no_encrypt.connect_toggled(move |c| a.encryption_toggled(c.is_active()));
        }
        {
            let a = a.clone();
            use_pass.connect_toggled(move |c| {
                a.st.borrow_mut().use_tpm = a.st.borrow().tpm_chip && !c.is_active();
                // A PIN and a passphrase are different secrets: start the fields again.
                a.pass.set_text("");
                a.again.set_text("");
                a.refresh();
            });
        }
        {
            let a = a.clone();
            back.connect_clicked(move |_| a.backward());
        }
        {
            let a = a.clone();
            next.connect_clicked(move |_| a.forward());
        }
        {
            let a = a.clone();
            window.connect_close_request(move |_| {
                if a.st.borrow().installing {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }

        a.show(0, false);
        window.present();
        a.shot();
        if a.preview.is_some() {
            a.auto();
        }
    }

    fn name(&self) -> &'static str {
        PAGES[self.cur.get()]
    }

    /// Switches to page `i`: the old page fades, the ridge marker moves, the new page builds in.
    fn show(self: &Rc<Self>, i: usize, animate: bool) {
        let from = self.cur.get();
        self.cur.set(i);
        let animate = animate && i != from && motion::animations_enabled();
        self.ridge.go_to(STOPS[i], animate);
        let a = self.clone();
        let switch = move || {
            let name = PAGES[i];
            let parts = if animate { a.pages.child_by_name(name).map(|p| motion::prepare(p.upcast_ref())) } else { None };
            a.pages.set_visible_child_name(name);
            if let Some(parts) = parts {
                motion::play(parts);
            }
        };
        match (animate, self.pages.visible_child()) {
            (true, Some(old)) => motion::exit(&old, switch),
            _ => switch(),
        }
        self.step_lbl.set_text(&if i < 4 { format!("{} of 4", i + 1) } else { String::new() });
        self.err.set_visible(false);
        self.refresh();
    }

    /// Buttons for the current page and state.
    fn refresh(self: &Rc<Self>) {
        let st = self.st.borrow();
        let name = self.name();
        self.back.set_visible(matches!(name, "where" | "encrypt" | "review"));
        self.next.set_visible(name != "installing");
        self.next.remove_css_class("ai-danger");
        let (text, ok) = match name {
            "welcome" => ("Continue", true),
            "where" => ("Continue", st.picked.is_some()),
            "encrypt" => {
                let pin = st.use_tpm;
                let problem = match (st.encrypt, pin) {
                    (false, _) => None,
                    (true, true) => backend::pin_problem(&self.pass.text(), &self.again.text()),
                    (true, false) => backend::passphrase_problem(&self.pass.text(), &self.again.text()),
                };
                self.pass.set_property("placeholder-text", if pin { "PIN" } else { "Passphrase" });
                self.use_pass.set_sensitive(st.encrypt);
                self.enc_sub.set_text(if pin {
                    "ArkaOS keeps your files and settings on an encrypted partition. When the computer starts, its TPM security chip unlocks it after you type this PIN, as long as Secure Boot is set up as it is now. At the end of the install you get a recovery key, for when the chip can't unlock it."
                } else {
                    "ArkaOS keeps your files and settings on an encrypted partition. They can be read only after this passphrase is typed when the computer starts. At the end of the install you get a recovery key, for when the passphrase is forgotten."
                });
                let typed = !self.pass.text().is_empty() || !self.again.text().is_empty();
                let show_problem = typed && problem.is_some();
                self.enc_note.set_text(match (problem, typed) {
                    (Some(p), true) => p,
                    _ if pin => "At least 6 characters. Digits are quickest to type; letters work too.",
                    _ => "At least 8 characters. A few unrelated words are easier to type and remember than a short password.",
                });
                self.enc_note.set_css_classes(&[if show_problem { "oobe-err" } else { "oobe-note" }]);
                ("Continue", problem.is_none())
            }
            "review" => {
                let erase = st.plan.as_ref().is_some_and(|p| p.mode == "erase");
                if erase {
                    self.next.add_css_class("ai-danger");
                }
                (if erase { "Erase and install" } else { "Install ArkaOS" }, st.plan.is_some())
            }
            "done" => ("Restart", !st.key_shown || st.key_saved),
            "failed" => ("Start over", true),
            _ => ("Continue", false),
        };
        self.next.set_label(text);
        self.next.set_sensitive(ok);
    }

    fn backward(self: &Rc<Self>) {
        let i = self.cur.get();
        if (1..=3).contains(&i) {
            self.show(i - 1, true);
        }
    }

    fn forward(self: &Rc<Self>) {
        match self.name() {
            "welcome" => {
                self.show(1, true);
                self.load_choices();
            }
            "where" => self.show(2, true),
            "encrypt" => {
                self.show(3, true);
                self.load_plan();
            }
            "review" => self.confirm_install(),
            "done" => {
                if let Err(e) = backend::restart() {
                    self.err.set_text(&format!("Could not restart ({e}). Hold the power button to turn the computer off."));
                    self.err.set_visible(true);
                }
            }
            "failed" => {
                {
                    let mut st = self.st.borrow_mut();
                    st.picked = None;
                    st.plan = None;
                }
                self.show(0, true);
            }
            _ => {}
        }
    }

    /// Asks the backend for every disk and every way to install on it.
    fn load_choices(self: &Rc<Self>) {
        clear(&self.where_body);
        self.where_body.append(&busy("Reading the disks…"));
        self.st.borrow_mut().picked = None;
        self.refresh();
        let (keymap, image) = (self.st.borrow().keymap.clone(), self.image.clone());
        let a = self.clone();
        glib::spawn_future_local(async move {
            let found = gio::spawn_blocking(move || match backend::probe() {
                Said::Ok(disks) => {
                    let mut choices = vec![];
                    for d in disks.iter().filter(|d| !d.in_use) {
                        let tag = d.path.trim_start_matches("/dev/").replace('/', "-");
                        let p = |mode: &str| backend::plan(&d.path, mode, true, false, &keymap, &image, &format!("/run/arka-install/choice-{tag}-{mode}.json"));
                        choices.extend(backend::choices(d, &p("free"), &p("empty"), &p("erase")));
                    }
                    Ok((disks, choices))
                }
                Said::Refused(m) | Said::Failed(m) => Err(m),
            })
            .await
            .unwrap_or_else(|_| Err("Reading the disks stopped unexpectedly.".into()));
            a.fill_choices(found);
        });
    }

    fn fill_choices(self: &Rc<Self>, found: Result<(Vec<Disk>, Vec<Choice>), String>) {
        clear(&self.where_body);
        let (disks, mut choices) = match found {
            Ok(v) => v,
            Err(m) => {
                self.where_body.append(&label(&m, "oobe-err"));
                return;
            }
        };
        // Usable installs first, then the ones that need room made, then erasing.
        choices.sort_by_key(|c| (c.destructive, c.refused.is_some()));
        if choices.iter().all(|c| c.refused.is_some()) {
            self.where_body.append(&label("No disk in this computer can take ArkaOS as it is.", "ai-disk"));
        }
        let mut group: Option<gtk4::ToggleButton> = None;
        let mut erase_heading = false;
        for (i, c) in choices.iter().enumerate() {
            if c.destructive && !erase_heading {
                erase_heading = true;
                let h = label("Or erase a disk", "ai-section");
                h.set_margin_top(sp(20));
                self.where_body.append(&h);
            }
            let inner = gtk4::Box::new(gtk4::Orientation::Vertical, sp(4));
            inner.append(&label(&c.title, "ai-card-title"));
            inner.append(&label(&c.disk_line, "ai-card-disk"));
            let text = label(c.refused.as_ref().unwrap_or(&c.sub), if c.refused.is_some() { "ai-card-why" } else { "ai-card-sub" });
            text.set_margin_top(sp(6));
            inner.append(&text);
            for w in &c.warnings {
                inner.append(&label(w, "ai-card-warn"));
            }
            if c.refused.is_some() {
                // Not a button: nothing to pick, only an explanation, kept fully readable.
                inner.add_css_class("ai-card");
                inner.add_css_class("ai-refused");
                self.where_body.append(&inner);
                if c.mode == "free" {
                    let how = gtk4::Expander::new(Some("How to make room next to Windows"));
                    how.add_css_class("ai-adv");
                    how.set_child(Some(&label(
                        "In Windows, right-click Start and open Disk Management. Right-click the C: drive, choose Shrink Volume, and enter at least 42000 as the amount to shrink in MB. Then restart into this installer.",
                        "ai-help",
                    )));
                    self.where_body.append(&how);
                }
                continue;
            }
            let card = gtk4::ToggleButton::new();
            card.add_css_class("ai-card");
            if c.destructive {
                card.add_css_class("ai-danger");
            }
            card.set_child(Some(&inner));
            card.set_group(group.as_ref());
            if group.is_none() {
                group = Some(card.clone());
            }
            let a = self.clone();
            card.connect_toggled(move |b| {
                if b.is_active() {
                    a.st.borrow_mut().picked = Some(i);
                    a.refresh();
                }
            });
            self.where_body.append(&card);
        }
        if choices.is_empty() {
            self.where_body.append(&label(
                "No disk was found that ArkaOS can install on. Disks smaller than 40 GB and the disk this installer runs from are not offered.",
                "oobe-sub",
            ));
        }
        {
            let mut st = self.st.borrow_mut();
            st.disks = disks;
            st.choices = choices;
        }
        self.refresh();
        if self.preview.is_some() {
            self.auto();
        }
    }

    fn encryption_toggled(self: &Rc<Self>, off: bool) {
        if !off {
            self.st.borrow_mut().encrypt = true;
            self.pass_box.set_sensitive(true);
            self.refresh();
            return;
        }
        let d = adw::AlertDialog::new(
            Some("Install without encryption?"),
            Some("Anyone who has this computer or its disk can then read the files on ArkaOS, for example if a laptop is lost or stolen."),
        );
        d.add_response("cancel", "Keep encryption");
        d.add_response("off", "Don't encrypt");
        d.set_response_appearance("off", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let a = self.clone();
        d.connect_response(None, move |_, r| {
            if r == "off" {
                a.st.borrow_mut().encrypt = false;
                a.pass_box.set_sensitive(false);
            } else {
                a.no_encrypt.set_active(false);
            }
            a.refresh();
        });
        d.present(Some(&self.window));
    }

    /// The final plan, with the choices from every page; what the review shows is what apply runs.
    fn load_plan(self: &Rc<Self>) {
        clear(&self.review_body);
        self.review_body.append(&busy("Checking the plan…"));
        self.st.borrow_mut().plan = None;
        self.refresh();
        let st = self.st.borrow();
        let Some(c) = st.picked.and_then(|i| st.choices.get(i)).cloned() else { return };
        let (encrypt, tpm, keymap, image) = (st.encrypt, st.use_tpm, st.keymap.clone(), self.image.clone());
        drop(st);
        let a = self.clone();
        glib::spawn_future_local(async move {
            let said = gio::spawn_blocking(move || backend::plan(&c.disk, c.mode, encrypt, tpm, &keymap, &image, PLAN_FILE))
                .await
                .unwrap_or_else(|_| Said::Failed("Planning stopped unexpectedly.".into()));
            a.fill_review(said);
        });
    }

    fn fill_review(self: &Rc<Self>, said: Said<Plan>) {
        clear(&self.review_body);
        let p = match said {
            Said::Ok(p) => p,
            Said::Refused(m) | Said::Failed(m) => {
                self.review_body.append(&label(&m, "oobe-err"));
                self.review_body.append(&label("Go back to pick another option.", "oobe-note"));
                return;
            }
        };
        // Grouped like a summary people already know from other installers: what happens to
        // the disk, what changes at start-up, and how ArkaOS is protected.
        let head = gtk4::Box::new(gtk4::Orientation::Vertical, sp(2));
        head.append(&label(&p.disk.name(), "ai-disk"));
        head.append(&label(&format!("{} · {}", backend::gb(p.disk.size_bytes), p.disk.path), "ai-card-disk"));
        self.review_body.append(&head);
        let mut warnings = p.warnings.clone();
        if backend::on_battery() {
            warnings.insert(0, "This computer is running on battery. Connect the charger first. If the computer switches off during the install, ArkaOS is left half-installed.".into());
        }
        if !warnings.is_empty() {
            self.review_body.append(&warnbox(&warnings));
        }
        let group = |title: &str| {
            let g = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
            g.set_margin_top(sp(14));
            g.append(&label(title, "ai-section"));
            g
        };
        let disk = group("Partitions");
        for line in p.review.iter().skip(1) {
            let kind = if line.starts_with("Creates") {
                "create"
            } else if line.starts_with("Erases") {
                "erase"
            } else {
                ""
            };
            disk.append(&dot_line(line, kind));
        }
        self.review_body.append(&disk);
        let start = group("Start-up");
        start.append(&dot_line(
            &format!("Adds the start-up entry “{}” to this computer and puts it first.", p.entry_label),
            "create",
        ));
        if p.mode == "free" && p.disk.has_windows {
            start.append(&dot_line("Adds Windows to the menu ArkaOS shows at start-up.", "create"));
        }
        self.review_body.append(&start);
        let security = group("Security");
        if p.encrypt {
            let keymap = self.st.borrow().keymap.clone();
            let what = if p.tpm { "PIN" } else { "passphrase" };
            if p.tpm {
                security.append(&dot_line("Encrypted. The TPM chip unlocks it at start-up with the PIN you chose, while Secure Boot stays as it is now.", "create"));
            } else {
                security.append(&dot_line("Encrypted with the passphrase you chose.", "create"));
            }
            security.append(&dot_line("A recovery key is made and shown once, at the end of the install.", "create"));
            security.append(&dot_line(&format!("Keyboard layout for the {what} at start-up: {}.", backend::keymap_name(&keymap)), ""));
        } else {
            security.append(&dot_line("Not encrypted.", "erase"));
        }
        self.review_body.append(&security);
        self.st.borrow_mut().plan = Some(p);
        self.refresh();
        if self.preview.is_some() {
            self.auto();
        }
    }

    fn confirm_install(self: &Rc<Self>) {
        let st = self.st.borrow();
        let Some(p) = st.plan.clone() else { return };
        drop(st);
        if p.mode != "erase" {
            self.install();
            return;
        }
        let d = adw::AlertDialog::new(
            Some(&format!("Erase {} ({})?", p.disk.name(), backend::gb(p.disk.size_bytes))),
            Some(&if p.disk.has_windows {
                format!("Everything on {} is deleted, including Windows and its files. This can't be undone.", p.disk.path)
            } else {
                format!("Everything on {} is deleted. This can't be undone.", p.disk.path)
            }),
        );
        d.add_response("cancel", "Cancel");
        d.add_response("erase", "Erase and install");
        d.set_response_appearance("erase", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let a = self.clone();
        d.connect_response(None, move |_, r| {
            if r == "erase" {
                a.install();
            }
        });
        d.present(Some(&self.window));
    }

    fn install(self: &Rc<Self>) {
        let passphrase = {
            let mut st = self.st.borrow_mut();
            st.installing = true;
            st.last_step = None;
            st.log.clear();
            st.encrypt.then(|| self.pass.text().to_string())
        };
        // The passphrase leaves the fields as soon as it is handed over.
        self.pass.set_text("");
        self.again.set_text("");
        for r in &self.step_rows {
            r.spinner.stop();
            r.mark.set_visible_child_name("wait");
            r.text.remove_css_class("now");
            r.text.remove_css_class("done");
            r.time.set_text("");
        }
        self.st.borrow_mut().step_start = None;
        self.log_lbl.set_text("");
        self.show(4, true);
        let (tx, rx) = mpsc::channel();
        backend::apply(PLAN_FILE.into(), passphrase, tx);
        let a = self.clone();
        glib::timeout_add_local(Duration::from_millis(120), move || {
            if let Some((k, t)) = a.st.borrow().step_start {
                a.step_rows[k].time.set_text(&mmss(t.elapsed()));
            }
            while let Ok(e) = rx.try_recv() {
                match e {
                    Event::Step(s) => a.mark_step(&s),
                    Event::Log(l) => {
                        let mut st = a.st.borrow_mut();
                        st.log.push(l);
                        let start = st.log.len().saturating_sub(14);
                        a.log_lbl.set_text(&st.log[start..].join("\n"));
                    }
                    Event::Done(o) => {
                        a.finished(Ok(o));
                        return glib::ControlFlow::Break;
                    }
                    Event::Failed(m) => {
                        a.finished(Err(m));
                        return glib::ControlFlow::Break;
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    fn mark_step(&self, step: &str) {
        // "mount" has no line of its own: the line before it stays current.
        if step == "mount" {
            return;
        }
        let shown = step;
        let Some(k) = STEPS.iter().position(|(s, _)| *s == shown) else { return };
        {
            let mut st = self.st.borrow_mut();
            st.last_step = Some(shown.to_string());
            if st.step_start.map(|(i, _)| i) != Some(k) {
                st.step_start = Some((k, std::time::Instant::now()));
            }
        }
        for (i, r) in self.step_rows.iter().enumerate() {
            r.text.remove_css_class("now");
            r.text.remove_css_class("done");
            if i < k {
                r.text.add_css_class("done");
                r.spinner.stop();
                r.mark.set_visible_child_name("done");
            } else if i == k {
                r.text.add_css_class("now");
                r.mark.set_visible_child_name("now");
                r.spinner.start();
            }
        }
    }

    fn finished(self: &Rc<Self>, r: Result<Outcome, String>) {
        {
            let mut st = self.st.borrow_mut();
            st.installing = false;
            st.step_start = None;
        }
        for row in &self.step_rows {
            row.spinner.stop();
        }
        match r {
            Ok(o) => {
                clear(&self.done_body);
                let checked = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
                checked.append(&label("Checked after installing", "ai-section"));
                let (free, windows, encrypt, tpm) = {
                    let st = self.st.borrow();
                    let p = st.plan.as_ref();
                    (p.is_some_and(|p| p.mode == "free"), p.is_some_and(|p| p.mode == "free" && p.disk.has_windows), st.encrypt, p.is_some_and(|p| p.tpm))
                };
                // Only next to existing partitions is there anything to have kept.
                if free && o.kept_unchanged {
                    checked.append(&dot_line("The partitions that were already on the disk are where they were, with the same size and type.", "create"));
                }
                if o.windows_boot_files_unchanged == Some(true) {
                    checked.append(&dot_line("Windows Boot Manager's files are unchanged.", "create"));
                }
                if o.fallback_unchanged == Some(false) {
                    checked.append(&dot_line(
                        "The disk's fallback start-up file (EFI\\BOOT\\BOOTX64.EFI) is now ArkaOS's. Firmware normally uses it only when it has no other start-up entry for this disk.",
                        "",
                    ));
                }
                if let Some(e) = &o.boot_entry {
                    let name = e.split_once(' ').map(|(_, n)| n).unwrap_or(e);
                    checked.append(&dot_line(&format!("The start-up entry “{name}” is in place."), "create"));
                }
                self.done_body.append(&checked);
                {
                    let mut st = self.st.borrow_mut();
                    st.key_shown = o.recovery_key.is_some();
                    st.key_saved = false;
                }
                if let Some(k) = &o.recovery_key {
                    let kb = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
                    kb.set_margin_top(sp(28));
                    kb.append(&label("Recovery key", "ai-section"));
                    // Two lines of four groups; the dashes are part of the key when it is typed.
                    let groups: Vec<&str> = k.split('-').collect();
                    let shown = if groups.len() == 8 { format!("{}-\n{}", groups[..4].join("-"), groups[4..].join("-")) } else { k.clone() };
                    let key = label(&shown, "ai-key");
                    key.set_selectable(true);
                    kb.append(&key);
                    kb.append(&label(
                        if tpm {
                            "ArkaOS asks for this key when the TPM chip can't unlock the disk: for example after Secure Boot is turned off or its keys change, or if the disk is moved to another computer. Type it with the dashes. Write it down or take a photo, and keep it away from this computer. It is shown only now."
                        } else {
                            "This key opens the disk if the passphrase is forgotten. Write it down or take a photo, and keep it away from this computer. It is shown only now."
                        },
                        "oobe-note",
                    ));
                    let saved = gtk4::CheckButton::with_label("I have written down or photographed the recovery key");
                    saved.add_css_class("oobe-tool");
                    let a = self.clone();
                    saved.connect_toggled(move |c| {
                        a.st.borrow_mut().key_saved = c.is_active();
                        a.refresh();
                    });
                    kb.append(&saved);
                    self.done_body.append(&kb);
                }
                let next = gtk4::Box::new(gtk4::Orientation::Vertical, sp(10));
                next.set_margin_top(sp(28));
                next.append(&label("After the restart", "ai-section"));
                if windows {
                    next.append(&dot_line(
                        "To start Windows, pick Windows in the menu that appears when the computer starts. Your computer's boot menu key also lists it; the key differs by maker.",
                        "",
                    ));
                    next.append(&dot_line(
                        "If the computer starts Windows straight away, without the ArkaOS menu, its firmware kept Windows first. Open the firmware settings and put ArkaOS first in the boot order; on some computers that list is inside the disk's own boot priorities.",
                        "",
                    ));
                }
                next.append(&dot_line(
                    if encrypt && tpm {
                        "ArkaOS asks for your PIN, then sets up your account the first time it starts."
                    } else if encrypt {
                        "ArkaOS asks for the disk passphrase, then sets up your account the first time it starts."
                    } else {
                        "ArkaOS sets up your account the first time it starts."
                    },
                    "",
                ));
                self.done_body.append(&next);
                self.show(5, true);
            }
            Err(m) => {
                self.fail_sub.set_text("ArkaOS was not installed. The installer's own message is below.");
                clear(&self.fail_body);
                let msg = label(&m, "ai-log");
                msg.add_css_class("ai-message");
                msg.set_selectable(true);
                self.fail_body.append(&msg);
                let last = self.st.borrow().last_step.clone();
                match last.as_deref() {
                    None | Some("check") => self.fail_body.append(&dot_line("Nothing was written to any disk.", "")),
                    Some(s) => {
                        let text = STEPS.iter().find(|(k, _)| *k == s).map(|(_, t)| *t).unwrap_or(s);
                        self.fail_body.append(&dot_line(&format!("It stopped at: {text}."), "erase"));
                    }
                }
                self.fail_body.append(&dot_line(&format!("The install log is at {}.", backend::LOG), ""));
                self.show(6, true);
            }
        }
    }

    /// Preview only: walks itself to the page named in ARKA_INSTALL_PREVIEW.
    fn auto(self: &Rc<Self>) {
        let Some(target) = self.preview.clone() else { return };
        let Some(t) = PAGES.iter().position(|p| *p == target) else { return };
        let a = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            let i = a.cur.get();
            if i >= t || i >= 4 {
                return;
            }
            match a.name() {
                "welcome" => a.forward(),
                "where" => {
                    let pick: usize = std::env::var("ARKA_INSTALL_PICK").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let cards: Vec<gtk4::ToggleButton> = {
                        let mut v = vec![];
                        let mut c = a.where_body.first_child();
                        while let Some(w) = c {
                            if let Ok(b) = w.clone().downcast::<gtk4::ToggleButton>() {
                                if b.is_sensitive() {
                                    v.push(b);
                                }
                            }
                            c = w.next_sibling();
                        }
                        v
                    };
                    let Some(card) = cards.get(pick) else { return };
                    card.set_active(true);
                    a.forward();
                    a.auto();
                }
                "encrypt" => {
                    a.pass.set_text("preview passphrase");
                    a.again.set_text("preview passphrase");
                    a.forward();
                }
                "review" => a.forward(),
                _ => {}
            }
        });
    }

    fn shot(&self) {
        let Ok(path) = std::env::var("ARKA_SHOT") else { return };
        let wait = std::env::var("ARKA_SHOT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(2500);
        let w: gtk4::Window = self.window.clone().upcast();
        glib::timeout_add_local_once(Duration::from_millis(wait), move || {
            let paintable = gtk4::WidgetPaintable::new(Some(&w));
            let snap = gtk4::Snapshot::new();
            paintable.snapshot(&snap, w.width() as f64, w.height() as f64);
            if let (Some(node), Some(r)) = (snap.to_node(), w.native().and_then(|n| n.renderer())) {
                let _ = r.render_texture(&node, None).save_to_png(&path);
            }
            if let Some(app) = w.application() {
                app.quit()
            }
        });
    }
}
