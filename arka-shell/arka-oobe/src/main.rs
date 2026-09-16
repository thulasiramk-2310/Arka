// arka-oobe — ArkaOS graphical first-run wizard (account creation).
// Runs as root, before any user exists, inside a `cage` kiosk compositor at
// first boot (see arka-oobe.service). Creates the account, sets autologin,
// switches to graphical.target and reboots into the desktop. Writes the same
// /var/lib/arkaos-firstboot-done flag the TUI fallback uses, so only one runs.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "org.arka.oobe";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

// ── account creation (root) ─────────────────────────────────────────────────

// Privileged account creation is delegated to /usr/libexec/arkaos-oobe-apply
// via a tight NOPASSWD sudo rule (see Containerfile). arka-oobe itself runs
// UNPRIVILEGED inside the arkasetup login session, so it must not touch /etc or
// run useradd directly. The password is passed on stdin, never in argv.
fn create_account(user: &str, pass: &str, autologin: bool) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("sudo")
        .args([
            "-n",
            "/usr/libexec/arkaos-oobe-apply",
            user,
            if autologin { "1" } else { "0" },
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("sudo arkaos-oobe-apply: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("apply: no stdin")?
        .write_all(format!("{pass}\n").as_bytes())
        .map_err(|e| format!("apply write: {e}"))?;
    let st = child.wait().map_err(|e| format!("apply wait: {e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("account setup failed ({st})"))
    }
}

fn valid_username(u: &str) -> bool {
    let mut c = u.chars();
    match c.next() {
        Some(f) if f.is_ascii_lowercase() => {}
        _ => return false,
    }
    u.len() <= 31 && u.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

// ── UI ───────────────────────────────────────────────────────────────────────

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let css = gtk4::CssProvider::new();
    css.load_from_data(
        "
        window, .oobe-bg { background-color: #07080e; }
        * { font-family: 'Inter', sans-serif; }
        .brand { font-size: 40px; font-weight: 800; color: #16c784; letter-spacing: 3px; }
        .tagline { font-size: 14px; color: #9aa4b2; letter-spacing: 3px; }
        .step-title { font-size: 26px; font-weight: 700; color: #f5f7fa; }
        .step-sub { font-size: 14px; color: #9aa4b2; }
        .err { color: #ff6b6e; font-size: 13px; }
        .cta { min-height: 46px; font-size: 15px; font-weight: 700; border-radius: 12px; }
        .card { background-color: #0f141c; border: 1px solid #1e2630; border-radius: 18px; }
        ",
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Set up ArkaOS")
        .default_width(560)
        .default_height(640)
        .build();
    window.fullscreen();

    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeft);
    stack.set_transition_duration(250);

    // shared inputs
    let user_row = adw::EntryRow::builder().title("Username").build();
    let pass_row = adw::PasswordEntryRow::builder().title("Password (8+ characters)").build();
    let conf_row = adw::PasswordEntryRow::builder().title("Confirm password").build();
    let auto_row = adw::SwitchRow::builder()
        .title("Log in automatically")
        .subtitle("Skip the password prompt at boot. Best for a personal machine.")
        .active(true)
        .build();

    // ── Step 1: welcome ───────────────────────────────────────────────────
    let welcome = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    welcome.set_valign(gtk4::Align::Center);
    welcome.set_halign(gtk4::Align::Center);
    let wbrand = gtk4::Label::new(Some("ARKAOS"));
    wbrand.add_css_class("brand");
    let wtag = gtk4::Label::new(Some("YOUR COMPUTER IS YOURS"));
    wtag.add_css_class("tagline");
    wtag.set_margin_bottom(40);
    let wtitle = gtk4::Label::new(Some("Let's set up your computer"));
    wtitle.add_css_class("step-title");
    wtitle.set_margin_top(24);
    let wsub = gtk4::Label::new(Some("Create your private account — it never leaves this device."));
    wsub.add_css_class("step-sub");
    wsub.set_margin_top(6);
    wsub.set_margin_bottom(36);
    let wbtn = gtk4::Button::with_label("Get started");
    wbtn.add_css_class("cta");
    wbtn.add_css_class("suggested-action");
    wbtn.set_size_request(240, -1);
    welcome.append(&wbrand);
    welcome.append(&wtag);
    welcome.append(&wtitle);
    welcome.append(&wsub);
    welcome.append(&wbtn);

    // ── Step 2: account ───────────────────────────────────────────────────
    let account = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    account.set_valign(gtk4::Align::Center);
    account.set_halign(gtk4::Align::Center);
    account.set_size_request(420, -1);
    let atitle = gtk4::Label::new(Some("Create your account"));
    atitle.add_css_class("step-title");
    atitle.set_halign(gtk4::Align::Start);
    let group = adw::PreferencesGroup::new();
    group.add(&user_row);
    group.add(&pass_row);
    group.add(&conf_row);
    let aerr = gtk4::Label::new(None);
    aerr.add_css_class("err");
    aerr.set_halign(gtk4::Align::Start);
    aerr.set_visible(false);
    let anext = gtk4::Button::with_label("Continue");
    anext.add_css_class("cta");
    anext.add_css_class("suggested-action");
    account.append(&atitle);
    account.append(&group);
    account.append(&aerr);
    account.append(&anext);

    // ── Step 3: preferences ───────────────────────────────────────────────
    let prefs = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    prefs.set_valign(gtk4::Align::Center);
    prefs.set_halign(gtk4::Align::Center);
    prefs.set_size_request(420, -1);
    let ptitle = gtk4::Label::new(Some("One last thing"));
    ptitle.add_css_class("step-title");
    ptitle.set_halign(gtk4::Align::Start);
    let pgroup = adw::PreferencesGroup::new();
    pgroup.add(&auto_row);
    let pcreate = gtk4::Button::with_label("Create account");
    pcreate.add_css_class("cta");
    pcreate.add_css_class("suggested-action");
    let pback = gtk4::Button::with_label("Back");
    pback.add_css_class("cta");
    let prow = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    prow.set_homogeneous(true);
    prow.append(&pback);
    prow.append(&pcreate);
    prefs.append(&ptitle);
    prefs.append(&pgroup);
    prefs.append(&prow);

    // ── Step 4: done ──────────────────────────────────────────────────────
    let done = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    done.set_valign(gtk4::Align::Center);
    done.set_halign(gtk4::Align::Center);
    let dcheck = gtk4::Label::new(Some("✓"));
    dcheck.add_css_class("brand");
    let dtitle = gtk4::Label::new(Some("All set!"));
    dtitle.add_css_class("step-title");
    dtitle.set_margin_top(16);
    let dsub = gtk4::Label::new(Some("Restarting into your desktop…"));
    dsub.add_css_class("step-sub");
    dsub.set_margin_top(8);
    done.append(&dcheck);
    done.append(&dtitle);
    done.append(&dsub);

    stack.add_named(&welcome, Some("welcome"));
    stack.add_named(&account, Some("account"));
    stack.add_named(&prefs, Some("prefs"));
    stack.add_named(&done, Some("done"));

    // ── navigation ─────────────────────────────────────────────────────────
    let s = stack.clone();
    wbtn.connect_clicked(move |_| s.set_visible_child_name("account"));

    let s = stack.clone();
    let (ur, pr, cr, er) = (user_row.clone(), pass_row.clone(), conf_row.clone(), aerr.clone());
    anext.connect_clicked(move |_| {
        let u = ur.text().to_string();
        let p = pr.text().to_string();
        let c = cr.text().to_string();
        let msg = if !valid_username(&u) {
            Some("Username: start with a lowercase letter; letters, numbers, _ or - only.")
        } else if p.chars().count() < 8 {
            Some("Password must be at least 8 characters.")
        } else if p != c {
            Some("Passwords don't match.")
        } else {
            None
        };
        match msg {
            Some(m) => { er.set_text(m); er.set_visible(true); }
            None => { er.set_visible(false); s.set_visible_child_name("prefs"); }
        }
    });

    let s = stack.clone();
    pback.connect_clicked(move |_| s.set_visible_child_name("account"));

    let created = Rc::new(RefCell::new(false));
    let s = stack.clone();
    let (ur, pr, ar) = (user_row.clone(), pass_row.clone(), auto_row.clone());
    let er = aerr.clone();
    let app_c = app.clone();
    let created_c = created.clone();
    pcreate.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        let u = ur.text().to_string();
        let p = pr.text().to_string();
        match create_account(&u, &p, ar.is_active()) {
            Ok(()) => {
                *created_c.borrow_mut() = true;
                s.set_visible_child_name("done");
                // give the "done" frame a moment to paint, then reboot
                let a = app_c.clone();
                glib::timeout_add_seconds_local(3, move || {
                    let _ = std::process::Command::new("systemctl").arg("reboot").spawn();
                    a.quit();
                    glib::ControlFlow::Break
                });
            }
            Err(e) => {
                er.set_text(&format!("Couldn't create the account: {e}"));
                er.set_visible(true);
                s.set_visible_child_name("account");
                btn.set_sensitive(true);
            }
        }
    });

    window.set_content(Some(&stack));
    window.present();
}
