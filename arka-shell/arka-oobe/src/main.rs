// arka-oobe — ArkaOS first-run setup.
// Runs unprivileged inside a `cage` kiosk compositor at first boot (see the
// Containerfile). Creates the account through a tight sudo helper, then
// reboots into the desktop. Writes the same /var/lib/arkaos-firstboot-done
// flag the TUI fallback uses, so only one runs.
//
// Shape: "hello world" decoding from noise, a light/dark choice, then only what is
// needed to start (name, password, how to sign in), a live privacy check read
// from arkad, and finally "hello <name>". No online account, no ads, no
// second setup.
//
// ARKA_OOBE_PREVIEW=<page> opens it windowed at a page for design review
// (hello, look, name, password, signin, privacy, working, bye) and never
// creates an account or reboots. ARKA_OOBE_THEME=dark previews the dark look.
// ARKA_OOBE_SHOT=<file.png> (preview only) renders the window to a PNG and
// quits.

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
use gtk4::cairo;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const APP_ID: &str = "org.arka.oobe";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

// ── account creation ──────────────────────────────────────────────────────────

// Privileged account creation is delegated to /usr/libexec/arkaos-oobe-apply
// via a tight NOPASSWD sudo rule (see Containerfile). arka-oobe itself runs
// UNPRIVILEGED inside the arkasetup login session, so it must not touch /etc or
// run useradd directly. The password is passed on stdin, never in argv.
fn create_account(user: &str, pass: &str, autologin: bool, dark: bool) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("sudo")
        .args([
            "-n",
            "/usr/libexec/arkaos-oobe-apply",
            user,
            if autologin { "1" } else { "0" },
            if dark { "dark" } else { "light" },
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
    u.len() <= 31
        && u.chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

// ── live privacy state (read-only, from arkad over the system bus) ────────────

/// One control's state as arkad reports it: Some(true) on, Some(false) off,
/// None while arkad is still starting or didn't answer.
type Reading = Option<bool>;

struct Privacy {
    dns: Reading,
    mac: Reading,
    hostname: Reading,
    ipv6: Reading,
    sandbox: Reading,
}

fn fresh_privacy_check(last: u64, now: u64) -> bool {
    last != 0 && now.checked_sub(last).is_some_and(|age| age < 90)
}

#[cfg(test)]
mod freshness_tests {
    #[test]
    fn reject_missing_future_and_stale_checks() {
        assert!(super::fresh_privacy_check(100, 189));
        for (last, now) in [(0, 100), (101, 100), (100, 190), (100, 1000)] {
            assert!(!super::fresh_privacy_check(last, now));
        }
    }
}

async fn read_privacy() -> Option<Privacy> {
    let bus = gio::bus_get_future(gio::BusType::System).await.ok()?;
    let reply = bus
        .call_future(
            Some("org.arka.arkad"),
            "/org/arka/arkad",
            "org.freedesktop.DBus.Properties",
            "GetAll",
            Some(&("org.arka.arkad",).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .ok()?;
    let dict = glib::VariantDict::new(Some(&reply.child_value(0)));
    let last = dict.lookup::<u64>("LastCheck").ok().flatten()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    if !fresh_privacy_check(last, now) { return None; }
    let s = |k: &str| dict.lookup::<String>(k).ok().flatten();
    let b = |k: &str| dict.lookup::<bool>(k).ok().flatten();
    // Same health rules arkad itself uses (arkad/src/state.rs).
    let dns = s("DnsStatus").map(DnsStatus::from).and_then(|d| match d {
        DnsStatus::Unknown(_) => None,
        d => Some(d.is_encrypted()),
    });
    let sandbox = match (s("SandboxStatus").map(SandboxStatus::from), s("BrowserSandbox").map(BrowserSandbox::from)) {
        (Some(SandboxStatus::Unknown(_)), _) | (_, Some(BrowserSandbox::Unknown(_))) | (None, _) | (_, None) => None,
        (Some(st), Some(br)) => Some(st.is_active() && br.is_isolated()),
    };
    Some(Privacy { dns, mac: b("MacRandomization"), hostname: b("HostnamePrivacy"), ipv6: b("Ipv6Privacy"), sandbox })
}

// ── look ──────────────────────────────────────────────────────────────────────

// Editorial and quiet: flat canvas, serif headings, one solid action colour.
// No cards, gradients, glows or blurred shapes. Every colour used is set here
// (both looks), so nothing from the host theme can leak through.
fn css(dark: bool) -> String {
    let (bg, fg, muted, faint, line, hover, pfg, phover, off) = if dark {
        ("#101114", "#f1f1f1", "#a6a6a6", "#8c8c8c", "#3b3e45", "#1c1e22", "#101114", "#d6d6d6", "#3b3e45")
    } else {
        ("#ffffff", "#111111", "#5c5c5c", "#737373", "#d4d4d4", "#f2f2f2", "#ffffff", "#2b2b2b", "#d9d9d9")
    };
    format!(
        "
window, stack, .oobe-root, .oobe-page {{ background-color: {bg}; color: {fg}; }}
* {{ font-family: 'Inter', 'Noto Sans', sans-serif; }}

.oobe-type {{ font-family: 'JetBrains Mono', 'Noto Sans Mono', monospace; font-size: 64px; font-weight: 400; color: {fg}; }}
.oobe-fade {{ opacity: 0; transition: opacity 700ms ease-out; }}
.oobe-fade.in {{ opacity: 1; }}
.oobe-hint {{ font-size: 15px; color: {faint}; }}
.oobe-mono {{ font-family: 'JetBrains Mono', 'Noto Sans Mono', monospace; font-size: 12px; color: {faint}; }}

.oobe-brand {{ font-size: 14px; font-weight: 600; color: {fg}; }}
.oobe-step {{ font-size: 13px; color: {muted}; }}
.oobe-seg {{ min-width: 30px; min-height: 3px; border-radius: 2px; background-color: {line}; }}
.oobe-seg.on {{ background-color: {fg}; }}

.oobe-title {{ font-family: 'Noto Serif', serif; font-size: 44px; font-weight: 500; color: {fg}; }}
.oobe-sub {{ font-size: 17px; color: {muted}; }}
.oobe-note {{ font-size: 14px; color: {faint}; }}
.oobe-err {{ font-size: 14px; color: {err}; }}

entry.oobe-field, passwordentry.oobe-field {{
  background: none; background-color: transparent; color: {fg};
  border: none; border-bottom: 1.5px solid {line}; border-radius: 0;
  box-shadow: none; outline: none; font-size: 24px; min-height: 54px; padding: 0 2px;
}}
entry.oobe-field:focus-within, passwordentry.oobe-field:focus-within {{
  border-bottom: 1.5px solid {fg}; box-shadow: none; outline: none;
}}
entry.oobe-field text, passwordentry.oobe-field text {{ background: none; color: {fg}; caret-color: {fg}; }}
entry.oobe-field text placeholder, passwordentry.oobe-field text placeholder {{ color: {faint}; }}
passwordentry.oobe-field image {{ color: {faint}; }}

button.oobe-primary {{
  background: none; background-color: {fg}; color: {pfg};
  border: none; border-radius: 999px; box-shadow: none;
  min-height: 50px; padding: 0 30px; font-size: 16px; font-weight: 600;
}}
button.oobe-primary:hover {{ background-color: {phover}; }}
button.oobe-primary:disabled {{ background-color: {off}; color: {pfg}; }}
button.oobe-back {{
  background: none; background-color: transparent; color: {fg};
  border: none; box-shadow: none; min-height: 50px; padding: 0 16px; font-size: 16px; border-radius: 999px;
}}
button.oobe-back:hover {{ background-color: {hover}; }}

button.oobe-tile {{
  background: none; background-color: transparent; border-radius: 16px; padding: 10px;
  border: 1.5px solid {line}; box-shadow: none; color: {fg};
}}
button.oobe-tile:hover {{ background-color: {hover}; }}
button.oobe-tile:checked {{ background-color: transparent; border: 2px solid {fg}; }}
button.oobe-tile:focus-visible {{ outline: 2px solid {faint}; outline-offset: 2px; }}
.oobe-tile-name {{ font-size: 16px; font-weight: 600; color: {fg}; }}

checkbutton.oobe-choice {{ padding: 14px 0; color: {fg}; }}
checkbutton.oobe-choice radio {{
  background: none; background-color: {bg}; border: 1.5px solid {faint};
  min-width: 18px; min-height: 18px; box-shadow: none; color: {pfg};
}}
checkbutton.oobe-choice radio:checked {{ background-color: {fg}; border-color: {fg}; }}
.oobe-choice-title {{ font-size: 18px; font-weight: 600; color: {fg}; }}
.oobe-choice-sub {{ font-size: 15px; color: {muted}; }}

.oobe-dot {{ min-width: 9px; min-height: 9px; border-radius: 5px; background-color: {line}; }}
.oobe-dot.ok {{ background-color: #12a06e; }}
.oobe-dot.warn {{ background-color: #d97706; }}
.oobe-row-title {{ font-size: 17px; font-weight: 600; color: {fg}; }}
.oobe-row-sub {{ font-size: 15px; color: {muted}; }}
.oobe-row-state {{ font-size: 14px; font-weight: 600; color: {ok}; }}
.oobe-row-state.off {{ color: {faint}; }}
.oobe-row-state.warn {{ color: {warn}; }}
.oobe-row-act {{ font-size: 14px; color: {warn}; }}
button.oobe-back:disabled {{ color: {faint}; opacity: 0.5; }}
",
        err = if dark { "#ff8a80" } else { "#b42318" },
        ok = if dark { "#3ddc97" } else { "#0e8a5f" },
        warn = if dark { "#fbbf5c" } else { "#b45309" },
    )
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.add_css_class(class);
    l.set_xalign(0.0);
    l.set_halign(gtk4::Align::Start);
    l.set_wrap(true);
    l
}

/// A setup page: title, subtitle, then `body`, in a centred column at most
/// 640px wide. It scrolls when the screen is short or text is enlarged, so
/// nothing ever ends up out of reach.
fn page(title: &gtk4::Label, sub: &gtk4::Label, body: &impl IsA<gtk4::Widget>) -> gtk4::ScrolledWindow {
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    col.add_css_class("oobe-page");
    sub.set_margin_top(14);
    body.as_ref().set_margin_top(40);
    col.append(title);
    col.append(sub);
    col.append(body);
    let clamp = adw::Clamp::builder()
        .maximum_size(640)
        .tightening_threshold(520)
        .child(&col)
        .valign(gtk4::Align::Center)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(24)
        .build();
    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&clamp)
        .build()
}

/// Reveals `text` in `l` as if decrypting it: every letter starts as
/// scrambled characters in faint Arka green, then locks into place left to
/// right. Monospace keeps the width steady while the glyphs churn.
fn decode_out(l: &gtk4::Label, text: &str, done: impl Fn() + 'static) {
    const NOISE: &[char] = &[
        'a', 'e', 'k', 'r', 'x', 'z', 'q', 'v', '0', '1', '3', '7', '8', '#', '%', '*', '+', '=', '/', '<', '>', '&',
    ];
    const TICK: u64 = 45; // ms between glyph changes
    const LEAD: u64 = 380; // scramble alone before the first letter locks
    const STEP: u64 = 95; // gap between letters locking
    let chars: Vec<char> = text.chars().collect();
    let started = std::time::Instant::now();
    let l = l.clone();
    glib::timeout_add_local(Duration::from_millis(TICK), move || {
        let t = started.elapsed().as_millis() as u64;
        let mut markup = String::new();
        let mut locked = 0;
        for (i, &c) in chars.iter().enumerate() {
            if c == ' ' || t >= LEAD + i as u64 * STEP {
                markup.push_str(&glib::markup_escape_text(&c.to_string()));
                locked += 1;
            } else {
                let n = NOISE[glib::random_int_range(0, NOISE.len() as i32) as usize];
                markup.push_str(&format!(
                    "<span foreground=\"#16c784\" fgalpha=\"60%\">{}</span>",
                    glib::markup_escape_text(&n.to_string())
                ));
            }
        }
        l.set_markup(&markup);
        if locked == chars.len() {
            done();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// The large monospace line the opening and closing messages decode into.
fn typed_line() -> (gtk4::Box, gtk4::Label) {
    let l = gtk4::Label::new(None);
    l.add_css_class("oobe-type");
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.set_halign(gtk4::Align::Center);
    row.append(&l);
    (row, l)
}

fn rounded(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// Small desktop mock-up for the light/dark tiles.
fn draw_mock(cr: &cairo::Context, w: f64, h: f64, dark: bool) {
    let rgb = |cr: &cairo::Context, c: u32| {
        cr.set_source_rgb(((c >> 16) & 255) as f64 / 255.0, ((c >> 8) & 255) as f64 / 255.0, (c & 255) as f64 / 255.0)
    };
    let (desk, bar, win, head, text, dock) = if dark {
        (0x16181d, 0x0d0e11, 0x23262d, 0x2c3038, 0x5a606b, 0x2c3038)
    } else {
        (0xe9edf2, 0xf9fafb, 0xffffff, 0xf1f3f6, 0xcdd3db, 0xffffff)
    };
    rounded(cr, 0.0, 0.0, w, h, 10.0);
    rgb(cr, desk);
    let _ = cr.fill();
    cr.rectangle(0.0, 0.0, w, 14.0);
    rgb(cr, bar);
    let _ = cr.fill();
    let (wx, wy, ww, wh) = (w * 0.14, 30.0, w * 0.72, h - 72.0);
    rounded(cr, wx, wy, ww, wh, 6.0);
    rgb(cr, win);
    let _ = cr.fill();
    cr.rectangle(wx, wy + 6.0, ww, 12.0);
    rounded(cr, wx, wy, ww, 18.0, 6.0);
    rgb(cr, head);
    let _ = cr.fill();
    for (i, f) in [0.62, 0.48, 0.55].iter().enumerate() {
        rounded(cr, wx + 14.0, wy + 32.0 + i as f64 * 14.0, ww * f, 6.0, 3.0);
        rgb(cr, text);
        let _ = cr.fill();
    }
    rounded(cr, wx + 14.0, wy + wh - 26.0, 46.0, 14.0, 7.0);
    cr.set_source_rgb(0.086, 0.78, 0.52);
    let _ = cr.fill();
    rounded(cr, w * 0.32, h - 26.0, w * 0.36, 16.0, 8.0);
    rgb(cr, dock);
    let _ = cr.fill();
}

// ── UI ────────────────────────────────────────────────────────────────────────

fn build_ui(app: &adw::Application) {
    let preview = std::env::var("ARKA_OOBE_PREVIEW").ok();
    let dark = Rc::new(Cell::new(preview.is_some() && std::env::var("ARKA_OOBE_THEME").as_deref() == Ok("dark")));

    let provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );
    let set_look: Rc<dyn Fn(bool)> = {
        let (provider, dark) = (provider.clone(), dark.clone());
        Rc::new(move |d: bool| {
            dark.set(d);
            adw::StyleManager::default().set_color_scheme(if d {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });
            provider.load_from_data(&css(d));
        })
    };
    set_look(dark.get());

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("ArkaOS Setup")
        .default_width(1280)
        .default_height(800)
        .build();
    if let Some((w, h)) = std::env::var("ARKA_OOBE_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        window.set_default_size(w, h);
    }
    if preview.is_none() {
        window.fullscreen();
    }

    let root = gtk4::Stack::new();
    root.add_css_class("oobe-root");
    root.set_transition_type(gtk4::StackTransitionType::Crossfade);
    root.set_transition_duration(600);

    // ── opening: "hello world" typed out ──────────────────────────────────
    let (hello_row, hello_lbl) = typed_line();
    let hint = gtk4::Label::new(Some("Press any key or click to begin"));
    hint.add_css_class("oobe-hint");
    hint.add_css_class("oobe-fade");
    let hello_box = gtk4::Box::new(gtk4::Orientation::Vertical, 40);
    hello_box.set_valign(gtk4::Align::Center);
    hello_box.set_halign(gtk4::Align::Center);
    hello_box.append(&hello_row);
    hello_box.append(&hint);
    hello_box.set_vexpand(true);
    let stack_note = gtk4::Label::new(Some("rust · gtk4 · bootc"));
    stack_note.add_css_class("oobe-mono");
    stack_note.set_margin_bottom(32);
    let hello_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    hello_page.append(&hello_box);
    hello_page.append(&stack_note);
    root.add_named(&hello_page, Some("hello"));

    // ── closing: "hello <name>" ───────────────────────────────────────────
    let (bye_row, bye_lbl) = typed_line();
    let bye_sub = gtk4::Label::new(Some("Restarting into your desktop…"));
    bye_sub.add_css_class("oobe-hint");
    bye_sub.add_css_class("oobe-fade");
    let bye_box = gtk4::Box::new(gtk4::Orientation::Vertical, 40);
    bye_box.set_valign(gtk4::Align::Center);
    bye_box.set_halign(gtk4::Align::Center);
    bye_box.append(&bye_row);
    bye_box.append(&bye_sub);
    root.add_named(&bye_box, Some("bye"));

    // ── setup frame: brand + progress / pages / actions ───────────────────
    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("oobe-page");

    let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    top.set_margin_top(36);
    top.set_margin_start(44);
    top.set_margin_end(44);
    let mark = gtk4::DrawingArea::new();
    mark.set_size_request(18, 18);
    mark.set_valign(gtk4::Align::Center);
    mark.set_draw_func(|_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);
        cr.set_source_rgb(0.086, 0.78, 0.52);
        cr.set_line_width(1.8);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.move_to(w / 2.0, 1.5);
        cr.line_to(w - 1.5, h - 2.5);
        cr.line_to(1.5, h - 2.5);
        cr.close_path();
        let _ = cr.stroke();
    });
    let brand = gtk4::Label::new(Some("ArkaOS Setup"));
    brand.add_css_class("oobe-brand");
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let segs_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    segs_box.set_valign(gtk4::Align::Center);
    let segs: Vec<gtk4::Box> = (0..5)
        .map(|_| {
            let s = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            s.add_css_class("oobe-seg");
            segs_box.append(&s);
            s
        })
        .collect();
    top.append(&mark);
    top.append(&brand);
    let step_lbl = gtk4::Label::new(None);
    step_lbl.add_css_class("oobe-step");
    step_lbl.set_margin_end(12);
    top.append(&spacer);
    top.append(&step_lbl);
    top.append(&segs_box);

    let pages = gtk4::Stack::new();
    pages.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
    pages.set_transition_duration(320);
    pages.set_vexpand(true);

    // look
    let tile = |name: &str, d: bool| -> gtk4::ToggleButton {
        let area = gtk4::DrawingArea::new();
        area.set_size_request(280, 170);
        area.set_draw_func(move |_, cr, w, h| draw_mock(cr, w as f64, h as f64, d));
        let n = label(name, "oobe-tile-name");
        n.set_margin_top(12);
        n.set_margin_start(4);
        n.set_margin_bottom(4);
        let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        col.append(&area);
        col.append(&n);
        let b = gtk4::ToggleButton::new();
        b.add_css_class("oobe-tile");
        b.set_child(Some(&col));
        b
    };
    let light_tile = tile("Light", false);
    let dark_tile = tile("Dark", true);
    dark_tile.set_group(Some(&light_tile));
    if dark.get() { dark_tile.set_active(true) } else { light_tile.set_active(true) }
    for (t, d) in [(&light_tile, false), (&dark_tile, true)] {
        let set_look = set_look.clone();
        t.connect_toggled(move |b| if b.is_active() { set_look(d) });
        // Keyboard: moving focus onto a look picks it, like a radio group, so
        // arrow + Enter chooses what is highlighted.
        t.connect_has_focus_notify(|b| if b.has_focus() { b.set_active(true) });
    }
    let look_body = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
    {
        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").expect("breakpoint"));
        bp.add_setter(&look_body, "orientation", Some(&gtk4::Orientation::Vertical.to_value()));
        window.add_breakpoint(bp);
    }
    look_body.append(&light_tile);
    look_body.append(&dark_tile);
    pages.add_named(
        &page(&label("Choose your look", "oobe-title"),
              &label("Your desktop starts in this style. You can switch anytime in Settings.", "oobe-sub"), &look_body),
        Some("look"),
    );

    // name
    let name_entry = gtk4::Entry::new();
    name_entry.add_css_class("oobe-field");
    name_entry.set_placeholder_text(Some("your name"));
    let name_body = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    name_body.append(&name_entry);
    name_body.append(&label("Lowercase letters and numbers, like alex or ram.", "oobe-note"));
    pages.add_named(
        &page(&label("What's your name?", "oobe-title"),
              &label("It becomes your username on this computer.", "oobe-sub"), &name_body),
        Some("name"),
    );

    // password
    let pass_entry = gtk4::PasswordEntry::new();
    pass_entry.add_css_class("oobe-field");
    pass_entry.set_placeholder_text(Some("Password"));
    pass_entry.set_show_peek_icon(true);
    let conf_entry = gtk4::PasswordEntry::new();
    conf_entry.add_css_class("oobe-field");
    conf_entry.set_placeholder_text(Some("Confirm password"));
    conf_entry.set_show_peek_icon(true);
    let pass_body = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    pass_body.append(&pass_entry);
    pass_body.append(&conf_entry);
    pass_body.append(&label("At least 8 characters.", "oobe-note"));
    pages.add_named(
        &page(&label("Choose a password", "oobe-title"),
              &label("You'll use it to sign in and to approve system changes.", "oobe-sub"), &pass_body),
        Some("password"),
    );

    // sign-in choice
    let choice = |title: &str, sub: &str| -> gtk4::CheckButton {
        let col = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        col.set_margin_start(10);
        col.append(&label(title, "oobe-choice-title"));
        col.append(&label(sub, "oobe-choice-sub"));
        let b = gtk4::CheckButton::new();
        b.add_css_class("oobe-choice");
        b.set_child(Some(&col));
        b
    };
    let ask = choice("Ask for my password",
                     "Anyone who turns on this computer needs your password to reach your desktop.");
    let auto = choice("Sign in automatically",
                      "Faster start-up. Anyone who turns on this computer goes straight to your desktop.");
    auto.set_group(Some(&ask));
    ask.set_active(true);
    for c in [&ask, &auto] {
        c.connect_has_focus_notify(|b| if b.has_focus() { b.set_active(true) });
    }
    let sign_body = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    sign_body.append(&ask);
    sign_body.append(&auto);
    pages.add_named(
        &page(&label("How should ArkaOS start?", "oobe-title"),
              &label("You can change this later in Settings.", "oobe-sub"), &sign_body),
        Some("signin"),
    );

    // live privacy check
    let priv_rows = gtk4::Box::new(gtk4::Orientation::Vertical, 22);
    let priv_note = label("Checking…", "oobe-note");
    priv_note.set_margin_top(30);
    let recheck = gtk4::Button::with_label("Check again");
    recheck.add_css_class("oobe-back");
    recheck.set_halign(gtk4::Align::Start);
    recheck.set_margin_top(10);
    recheck.set_visible(false);
    let priv_body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    priv_body.append(&priv_rows);
    priv_body.append(&priv_note);
    priv_body.append(&recheck);
    pages.add_named(
        &page(&label("Your privacy settings", "oobe-title"),
              &label("ArkaOS sets these up on its own. Here's how they stand right now.", "oobe-sub"),
              &priv_body),
        Some("privacy"),
    );

    // working
    let spinner = gtk4::Spinner::new();
    spinner.set_size_request(28, 28);
    spinner.set_halign(gtk4::Align::Start);
    let work_body = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    work_body.append(&spinner);
    pages.add_named(
        &page(&label("Setting things up", "oobe-title"),
              &label("Creating your account. This only takes a moment.", "oobe-sub"), &work_body),
        Some("working"),
    );

    // actions
    let err = label("", "oobe-err");
    err.set_visible(false);
    err.set_valign(gtk4::Align::Center);
    let back = gtk4::Button::with_label("Back");
    back.add_css_class("oobe-back");
    let next = gtk4::Button::with_label("Continue");
    next.add_css_class("oobe-primary");
    let bottom = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    bottom.set_margin_bottom(40);
    bottom.set_margin_start(44);
    bottom.set_margin_end(44);
    let bspace = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    bspace.set_hexpand(true);
    bottom.append(&err);
    bottom.append(&bspace);
    bottom.append(&back);
    bottom.append(&next);

    frame.append(&top);
    frame.append(&pages);
    frame.append(&bottom);
    root.add_named(&frame, Some("setup"));

    // ── navigation ────────────────────────────────────────────────────────
    const ORDER: &[&str] = &["look", "name", "password", "signin", "privacy", "working"];
    let cur = Rc::new(Cell::new(0usize));

    // Privacy rows: Checking… while arkad is asked (it may still be starting),
    // then Configured / Needs attention, or Unavailable if it never answers.
    #[derive(Clone, Copy, PartialEq)]
    enum Row { Checking, Configured, Attention, Unavailable }
    let render_privacy = {
        let (rows, note, recheck) = (priv_rows.clone(), priv_note.clone(), recheck.clone());
        move |states: [Row; 5], answered: bool| {
            while let Some(c) = rows.first_child() {
                rows.remove(&c);
            }
            // Each line says what the setting does; only the status column says
            // whether it is in place. "Configured" is not a promise of protection.
            let items: [(&str, &str); 5] = [
                ("Encrypted DNS", "Requires system DNS lookups to be encrypted."),
                ("Private network address", "Uses a random hardware address on network connections."),
                ("Generic device name", "Uses the generic network name \"arka\" instead of yours."),
                ("Temporary IPv6 addresses", "Uses temporary IPv6 addresses that change over time."),
                ("Sandboxed browser", "Opens Firefox in a sandbox that sees only your Downloads folder."),
            ];
            for ((t, s), st) in items.into_iter().zip(states) {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
                let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                dot.add_css_class("oobe-dot");
                dot.set_valign(gtk4::Align::Start);
                dot.set_margin_top(8);
                match st {
                    Row::Configured => dot.add_css_class("ok"),
                    Row::Attention => dot.add_css_class("warn"),
                    _ => {}
                }
                let col = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
                col.set_hexpand(true);
                col.append(&label(t, "oobe-row-title"));
                col.append(&label(s, "oobe-row-sub"));
                if st == Row::Attention || (st == Row::Unavailable && answered) {
                    col.append(&label("Review it in Privacy settings after setup.", "oobe-row-act"));
                }
                let state = gtk4::Label::new(Some(match st {
                    Row::Checking => "Checking…",
                    Row::Configured => "Configured",
                    Row::Attention => "Needs attention",
                    Row::Unavailable => "Unavailable",
                }));
                state.add_css_class("oobe-row-state");
                match st {
                    Row::Configured => {}
                    Row::Attention => state.add_css_class("warn"),
                    _ => state.add_css_class("off"),
                }
                state.set_valign(gtk4::Align::Start);
                state.set_margin_top(2);
                row.append(&dot);
                row.append(&col);
                row.append(&state);
                rows.append(&row);
            }
            let checking = states.contains(&Row::Checking);
            recheck.set_visible(!checking && states.iter().any(|s| *s != Row::Configured));
            note.set_text(if checking {
                "Checking the ArkaOS privacy service…"
            } else if answered {
                "Checked just now. This shows what ArkaOS has set up, and covers only what's listed here."
            } else {
                "Couldn't read the privacy status. You can finish setup and check Privacy settings afterwards."
            });
        }
    };

    // Keep checks fresh while this page is visible. `gen` cancels an old poll.
    // Unavailable data gets a short initialization grace period.
    let gen = Rc::new(Cell::new(0u32));
    let check_privacy: Rc<dyn Fn()> = {
        let (render, gen, cur) = (render_privacy.clone(), gen.clone(), cur.clone());
        Rc::new(move || {
            let my = gen.get().wrapping_add(1);
            gen.set(my);
            render([Row::Checking; 5], false);
            let (render, gen, cur) = (render.clone(), gen.clone(), cur.clone());
            glib::spawn_future_local(async move {
                const TRIES: u32 = 5;
                let mut attempt = 0u32;
                loop {
                    attempt = attempt.saturating_add(1);
                    let last = attempt >= TRIES;
                    let p = read_privacy().await;
                    if gen.get() != my || ORDER[cur.get()] != "privacy" {
                        return;
                    }
                    let to_row = |r: Reading| match r {
                        Some(true) => Row::Configured,
                        Some(false) => Row::Attention,
                        None if last => Row::Unavailable,
                        None => Row::Checking,
                    };
                    let states = match &p {
                        Some(p) => [to_row(p.dns), to_row(p.mac), to_row(p.hostname), to_row(p.ipv6), to_row(p.sandbox)],
                        None => [to_row(None); 5],
                    };
                    render(states, p.is_some());
                    glib::timeout_future(Duration::from_secs(2)).await;
                }
            });
        })
    };
    {
        let c = check_privacy.clone();
        recheck.connect_clicked(move |_| c());
    }

    let show: Rc<dyn Fn(usize)> = {
        let (cur, pages, segs) = (cur.clone(), pages.clone(), segs.clone());
        let (back, next, err, spinner) = (back.clone(), next.clone(), err.clone(), spinner.clone());
        let (name_entry, pass_entry, light_tile, dark_tile, dark) =
            (name_entry.clone(), pass_entry.clone(), light_tile.clone(), dark_tile.clone(), dark.clone());
        let (check_privacy, step_lbl, gen) = (check_privacy.clone(), step_lbl.clone(), gen.clone());
        Rc::new(move |i: usize| {
            cur.set(i);
            let name = ORDER[i];
            pages.set_visible_child_name(name);
            for (k, s) in segs.iter().enumerate() {
                if k <= i { s.add_css_class("on") } else { s.remove_css_class("on") }
            }
            err.set_visible(false);
            let busy = name == "working";
            step_lbl.set_visible(!busy);
            step_lbl.set_text(&format!("Step {} of {}", i + 1, ORDER.len() - 1));
            // Back stays in place on every step (just disabled on the first)
            // so Continue never moves.
            back.set_visible(!busy);
            back.set_sensitive(i > 0);
            gen.set(gen.get().wrapping_add(1));
            next.set_visible(!busy);
            next.set_sensitive(true);
            next.set_label(if name == "privacy" { "Finish setup" } else { "Continue" });
            spinner.set_spinning(busy);
            match name {
                "look" => { if dark.get() { dark_tile.grab_focus() } else { light_tile.grab_focus() }; }
                "name" => { name_entry.grab_focus(); }
                "password" => { pass_entry.grab_focus(); }
                "privacy" => {
                    check_privacy();
                    next.grab_focus();
                }
                _ => { next.grab_focus(); }
            }
        })
    };

    let fail = {
        let err = err.clone();
        move |msg: &str| { err.set_text(msg); err.set_visible(true); }
    };

    let finish: Rc<dyn Fn(String)> = {
        let (root, bye_lbl, bye_sub, app) = (root.clone(), bye_lbl.clone(), bye_sub.clone(), app.clone());
        let reboot = preview.is_none();
        Rc::new(move |user: String| {
            root.set_visible_child_name("bye");
            let (bye_sub, app) = (bye_sub.clone(), app.clone());
            let l = bye_lbl.clone();
            glib::timeout_add_local_once(Duration::from_millis(700), move || {
                decode_out(&l, &format!("hello {user}"), move || {
                    bye_sub.add_css_class("in");
                    if reboot {
                        let app = app.clone();
                        glib::timeout_add_local_once(Duration::from_millis(2600), move || {
                            let _ = std::process::Command::new("systemctl").arg("reboot").spawn();
                            app.quit();
                        });
                    }
                });
            });
        })
    };

    let advance: Rc<dyn Fn()> = {
        let (cur, show, fail, finish) = (cur.clone(), show.clone(), fail.clone(), finish.clone());
        let (name_entry, pass_entry, conf_entry, ask, dark) =
            (name_entry.clone(), pass_entry.clone(), conf_entry.clone(), ask.clone(), dark.clone());
        let next_btn = next.clone();
        let preview = preview.is_some();
        let busy = Rc::new(Cell::new(false));
        Rc::new(move || match ORDER[cur.get()] {
            "look" => show(1),
            "name" => {
                if valid_username(&name_entry.text()) { show(2) } else {
                    fail("Start with a lowercase letter, then use letters, numbers, _ or -.");
                }
            }
            "password" => {
                let (p, c) = (pass_entry.text().to_string(), conf_entry.text().to_string());
                if p.chars().count() < 8 { fail("Use at least 8 characters.") }
                else if p != c { fail("The passwords don't match.") }
                else { show(3) }
            }
            "signin" => show(4),
            "privacy" => {
                if busy.replace(true) { return; }
                next_btn.set_sensitive(false);
                show(5);
                let user = name_entry.text().to_string();
                let (u, p, autologin, d) = (user.clone(), pass_entry.text().to_string(), !ask.is_active(), dark.get());
                let (show, busy, fail, finish) = (show.clone(), busy.clone(), fail.clone(), finish.clone());
                glib::spawn_future_local(async move {
                    let res = if preview {
                        Ok(())
                    } else {
                        gio::spawn_blocking(move || create_account(&u, &p, autologin, d))
                            .await
                            .unwrap_or_else(|_| Err("account helper crashed".into()))
                    };
                    match res {
                        Ok(()) => finish(user),
                        Err(e) => {
                            busy.set(false);
                            show(4);
                            fail(&format!("Couldn't create the account: {e}"));
                        }
                    }
                });
            }
            _ => {}
        })
    };

    {
        let a = advance.clone();
        next.connect_clicked(move |_| a());
    }
    {
        let (cur, show) = (cur.clone(), show.clone());
        back.connect_clicked(move |_| { let i = cur.get(); if i > 0 { show(i - 1) } });
    }
    {
        let a = advance.clone();
        name_entry.connect_activate(move |_| a());
        let c = conf_entry.clone();
        pass_entry.connect_activate(move |_| { c.grab_focus(); });
        let a = advance.clone();
        conf_entry.connect_activate(move |_| a());
    }

    // Keyboard: Enter continues from any step (text fields already do this
    // through `activate`; plain buttons like Back keep their own Enter), and
    // Escape goes back.
    {
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let (a, w, root, cur, show, next) =
            (advance.clone(), window.clone(), root.clone(), cur.clone(), show.clone(), next.clone());
        keys.connect_key_pressed(move |_, key, _, _| {
            use gtk4::gdk::Key;
            if root.visible_child_name().as_deref() != Some("setup") || !next.is_visible() {
                return glib::Propagation::Proceed;
            }
            let focus = gtk4::prelude::GtkWindowExt::focus(&w);
            match key {
                Key::Return | Key::KP_Enter => {
                    let own_enter = focus.as_ref().is_some_and(|f| {
                        f.is::<gtk4::Text>() || (f.is::<gtk4::Button>() && !f.is::<gtk4::ToggleButton>())
                    });
                    if own_enter || !next.is_sensitive() {
                        return glib::Propagation::Proceed;
                    }
                    a();
                    glib::Propagation::Stop
                }
                Key::Escape if cur.get() > 0 => {
                    show(cur.get() - 1);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(keys);
    }

    window.set_content(Some(&root));

    // ── start ─────────────────────────────────────────────────────────────
    let begun = Rc::new(Cell::new(false));
    let begin: Rc<dyn Fn()> = {
        let (root, show, begun) = (root.clone(), show.clone(), begun.clone());
        Rc::new(move || {
            if begun.replace(true) { return; }
            root.set_visible_child_name("setup");
            show(0);
        })
    };

    match preview.as_deref() {
        None | Some("hello") => {
            root.set_visible_child_name("hello");
            let hint = hint.clone();
            glib::timeout_add_local_once(Duration::from_millis(600), move || {
                decode_out(&hello_lbl, "hello world", move || hint.add_css_class("in"));
            });
            let keys = gtk4::EventControllerKey::new();
            let b = begin.clone();
            keys.connect_key_pressed(move |_, _, _, _| { b(); glib::Propagation::Stop });
            hello_box.add_controller(keys);
            let click = gtk4::GestureClick::new();
            let b = begin.clone();
            click.connect_released(move |_, _, _, _| b());
            hello_box.add_controller(click);
            hello_box.set_focusable(true);
            hello_box.grab_focus();
        }
        Some("bye") => {
            root.set_transition_duration(0);
            finish("ram".into());
        }
        Some(pg) => {
            root.set_transition_duration(0);
            pages.set_transition_duration(0);
            root.set_visible_child_name("setup");
            let i = ORDER.iter().position(|n| *n == pg).unwrap_or(0);
            if i >= 1 { name_entry.set_text("ram"); }
            show(i);
        }
    }
    window.present();

    if let (Some(_), Ok(path)) = (preview.as_deref(), std::env::var("ARKA_OOBE_SHOT")) {
        let w = window.clone();
        let app = app.clone();
        let wait = std::env::var("ARKA_OOBE_SHOT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(1800);
        glib::timeout_add_local_once(Duration::from_millis(wait), move || {
            let paintable = gtk4::WidgetPaintable::new(Some(&w));
            let snap = gtk4::Snapshot::new();
            paintable.snapshot(&snap, w.width() as f64, w.height() as f64);
            if let (Some(node), Some(r)) = (snap.to_node(), w.native().and_then(|n| n.renderer())) {
                let _ = r.render_texture(&node, None).save_to_png(&path);
            }
            app.quit();
        });
    }
}
