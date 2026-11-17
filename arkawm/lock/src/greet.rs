//! `arka-lock --greeter`: the sign-in screen. The lock screen's card (clock, wallpaper, glass
//! card, password box, power buttons) started by greetd before anyone has signed in, inside cage.
//! It adds a choice of account when there is more than one, and a choice of desktop session
//! (ArkaWM or Plasma) at the bottom left. greetd checks the password with PAM and starts the
//! session once this screen has gone (see greetd.rs).
//!
//! The account and session last used are remembered in /var/cache/arka-greet/state. Until then
//! the ones first-boot setup wrote for the previous login screen are used.

use super::*;
use crate::greetd::{self, Failure};

const STATE: &str = "/var/cache/arka-greet/state";
/// What first-boot setup writes for the previous login screen (SDDM).
const SDDM_STATE: &str = "/var/lib/sddm/state.conf";
const SESSIONS: &str = "/usr/share/wayland-sessions";
/// The account first-boot setup runs in; it never signs in here.
const SETUP_ACCOUNT: &str = "arkasetup";

/// A desktop session that can be started, from its .desktop file.
#[derive(Clone, Debug, PartialEq)]
struct Session {
    /// The file's name, "arkawm.desktop": what is remembered.
    file: String,
    name: String,
    command: Vec<String>,
    /// DesktopNames, "ArkaWM" or "KDE".
    desktops: String,
}

/// The accounts people sign in to: user ids from 1000, a real shell, not the setup account.
fn accounts(passwd: &str) -> Vec<Person> {
    let mut found: Vec<Person> = passwd
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            let (login, uid, gecos, shell) = (*fields.first()?, fields.get(2)?.parse::<u32>().ok()?, *fields.get(4)?, *fields.get(6)?);
            let real_shell = !shell.ends_with("nologin") && !shell.ends_with("/false");
            if !(1000..60000).contains(&uid) || !real_shell || login == SETUP_ACCOUNT {
                return None;
            }
            let full = gecos.split(',').next().unwrap_or("").trim();
            Some(Person { login: login.to_string(), shown: if full.is_empty() { login.to_string() } else { full.to_string() } })
        })
        .collect();
    found.sort_by(|a, b| a.login.cmp(&b.login));
    found
}

/// One session file's entry, or None when it is hidden or has nothing to run.
fn session(file: &str, text: &str) -> Option<Session> {
    let mut entry = false;
    let mut values = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
        } else if entry {
            if let Some((key, value)) = line.split_once('=') {
                values.entry(key.trim().to_string()).or_insert_with(|| value.trim().to_string());
            }
        }
    }
    let yes = |key: &str| values.get(key).is_some_and(|v| v == "true");
    if yes("Hidden") || yes("NoDisplay") {
        return None;
    }
    // field codes (%f, %U ...) mean nothing for a session
    let command: Vec<String> = values.get("Exec")?.split_whitespace().filter(|part| !part.starts_with('%')).map(String::from).collect();
    if command.is_empty() {
        return None;
    }
    Some(Session {
        file: file.to_string(),
        name: values.get("Name").cloned().unwrap_or_else(|| file.trim_end_matches(".desktop").to_string()),
        command,
        desktops: values.get("DesktopNames").cloned().unwrap_or_default().trim_end_matches(';').to_string(),
    })
}

/// The sessions installed, ArkaWM first.
fn sessions() -> Vec<Session> {
    let mut found: Vec<Session> = std::fs::read_dir(SESSIONS)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().to_string();
            if !file.ends_with(".desktop") {
                return None;
            }
            session(&file, &std::fs::read_to_string(entry.path()).ok()?)
        })
        .collect();
    found.sort_by_key(|s| (s.file != "arkawm.desktop", s.name.to_lowercase()));
    found
}

/// The account and session file used last: ours, else the previous login screen's.
fn remembered(ours: &str, sddm: &str) -> (Option<String>, Option<String>) {
    let mine = parse(ours);
    if let Some(user) = mine.get("user") {
        return (Some(user.clone()), mine.get("session").cloned());
    }
    // SDDM: "[Last]" then User= and Session=/usr/share/wayland-sessions/plasma.desktop
    let sddm = parse(sddm);
    let file = sddm.get("Session").and_then(|path| path.rsplit('/').next()).map(String::from);
    (sddm.get("User").cloned(), file)
}

fn remember(user: &str, session: &str) {
    let text = format!("# The account and session last signed in to, for the sign-in screen.\nuser = {user}\nsession = {session}\n");
    if let Err(err) = std::fs::write(STATE, text) {
        eprintln!("arka-lock --greeter: could not remember the last sign-in in {STATE}: {err}");
    }
}

/// The environment a session gets beside what PAM sets: which desktop it is.
fn session_env(session: &Session) -> Vec<String> {
    let first = session.desktops.split(';').next().unwrap_or_default();
    let mut env = vec!["XDG_SESSION_TYPE=wayland".to_string()];
    if !first.is_empty() {
        env.push(format!("XDG_CURRENT_DESKTOP={}", session.desktops));
        env.push(format!("XDG_SESSION_DESKTOP={}", first.to_lowercase()));
    }
    env
}

/// Ask greetd. Debug builds started outside greetd stand in ARKA_LOCK_TEST_PASSWORD, so the
/// screen can be tried in a nested session.
fn ask_greetd(user: &str, password: &str, command: &[String], env: &[String]) -> Result<(), Failure> {
    #[cfg(debug_assertions)]
    if std::env::var_os("GREETD_SOCK").is_none() {
        if let Ok(test) = std::env::var("ARKA_LOCK_TEST_PASSWORD") {
            std::thread::sleep(std::time::Duration::from_millis(600));
            eprintln!("arka-lock --greeter (test): would start {command:?} with {env:?}");
            return if password == test { Ok(()) } else { Err(Failure::Refused(None)) };
        }
    }
    greetd::sign_in(user, password, command, env)
}

pub fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("org.arka.Greeter")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(start);
    app.run_with_args::<&str>(&[])
}

fn css(look: &Look) -> String {
    format!(
        r#"
        .chip {{ min-height: 36px; padding: 0 16px 0 12px; border-radius: 999px; background: {glass}; color: {text};
                 border: 1px solid {hair}; box-shadow: 0 6px 18px {drop_near}; font-size: 13px; font-weight: 500;
                 transition: background-color 160ms cubic-bezier(0.16,1,0.3,1), border-color 160ms cubic-bezier(0.16,1,0.3,1); }}
        .chip:hover {{ background: {glass_hover}; border-color: {accent_line}; }}
        .chip:focus-visible {{ border-color: {accent}; box-shadow: 0 0 0 3px {accent_ring}; }}
        .switch {{ margin-top: 14px; min-height: 30px; padding: 0 12px; border-radius: 999px; background: none; border: none;
                   box-shadow: none; color: {dim}; font-size: 13px; }}
        .switch:hover {{ color: {text}; background: {well}; }}
        popover.pick > contents {{ background: {panel}; border: 1px solid {hair}; border-radius: 16px; padding: 6px;
                                   box-shadow: 0 18px 40px {drop}; }}
        popover.pick > arrow {{ background: {panel}; border: 1px solid {hair}; }}
        popover.pick button {{ min-height: 36px; padding: 0 14px; border-radius: 10px; background: none; border: none;
                               box-shadow: none; color: {text}; font-size: 14px; }}
        popover.pick button:hover {{ background: {well}; }}
        popover.pick button.current {{ color: {accent}; font-weight: 600; }}
        "#,
        glass = look.rgba("panel", 0.52),
        glass_hover = look.rgba("panel", 0.72),
        panel = look.rgba("panel", 0.96),
        text = look.get("text"),
        dim = look.get("text_dim"),
        hair = look.rgba("text", 0.16),
        well = look.rgba("background", 0.40),
        drop = look.rgba("background", 0.42),
        drop_near = look.rgba("background", 0.28),
        accent = look.get("accent"),
        accent_line = look.rgba("accent", 0.45),
        accent_ring = look.rgba("accent", 0.28),
    )
}

/// A list to pick from, shown above or below `anchor`; `chosen` is told the index picked.
fn picker(anchor: &impl IsA<gtk::Widget>, names: &[String], current: usize, chosen: impl Fn(usize) + 'static) -> gtk::Popover {
    let popover = gtk::Popover::builder().css_classes(["pick"]).has_arrow(false).build();
    popover.set_parent(anchor);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let chosen = Rc::new(chosen);
    for (i, name) in names.iter().enumerate() {
        let button = gtk::Button::builder().label(name).build();
        if let Some(label) = button.child().and_downcast::<gtk::Label>() {
            label.set_xalign(0.0);
        }
        if i == current {
            button.add_css_class("current");
        }
        let (chosen, popover) = (chosen.clone(), popover.clone());
        button.connect_clicked(move |_| {
            popover.popdown();
            chosen(i);
        });
        column.append(&button);
    }
    popover.set_child(Some(&column));
    popover
}

fn start(app: &gtk::Application) {
    let look = Rc::new(Look::read());
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&format!("{}{}", look.css(), css(&look)));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let people = Rc::new(accounts(&std::fs::read_to_string("/etc/passwd").unwrap_or_default()));
    let choices = Rc::new(sessions());
    let (last_user, last_session) =
        remembered(&std::fs::read_to_string(STATE).unwrap_or_default(), &std::fs::read_to_string(SDDM_STATE).unwrap_or_default());
    let who = Rc::new(Cell::new(people.iter().position(|p| Some(&p.login) == last_user.as_ref()).unwrap_or(0)));
    let which = Rc::new(Cell::new(choices.iter().position(|s| Some(&s.file) == last_session.as_ref()).unwrap_or(0)));

    let height = gdk::Display::default()
        .and_then(|d| d.monitors().item(0))
        .and_then(|m| m.downcast::<gdk::Monitor>().ok())
        .map(|m| m.geometry().height())
        .unwrap_or(800);
    let first = people.get(who.get()).map(|p| Person { login: p.login.clone(), shown: p.shown.clone() });
    let shown = first.unwrap_or(Person { login: String::new(), shown: "ArkaOS".into() });
    let soft: Rc<RefCell<Option<gtk::cairo::ImageSurface>>> = Rc::default();
    let (window, time, date, built) = screen(app, &look, &shown, &soft, height, true);
    let Some(form) = built else { return };
    window.set_application(Some(app));
    window.set_title(Some("Sign in"));
    window.fullscreen();
    form.go.set_tooltip_text(Some("Sign in"));
    form.go.update_property(&[gtk::accessible::Property::Label("Sign in")]);

    // Another account, when there is more than one.
    if people.len() > 1 {
        let switch = gtk::Button::builder().label("Switch account").css_classes(["switch"]).halign(gtk::Align::Center).focus_on_click(false).build();
        form.body.append(&switch);
        let (people, who, form_ref) = (people.clone(), who.clone(), form.clone());
        switch.connect_clicked(move |switch| {
            let names: Vec<String> = people.iter().map(|p| p.shown.clone()).collect();
            let (people, who, form) = (people.clone(), who.clone(), form_ref.clone());
            let popover = picker(switch, &names, who.get(), move |i| {
                who.set(i);
                form.avatar.set_text(&initial(&people[i].shown));
                form.name.set_text(&people[i].shown);
                form.entry.set_text("");
                idle_note(&form);
                form.entry.grab_focus();
            });
            popover.connect_closed(|popover| popover.unparent());
            popover.popup();
        });
    }

    // The session, bottom left; the battery moves beside the power buttons to make room.
    if !choices.is_empty() {
        let chip_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        chip_row.append(&look.icon("squares-four-fill", 16));
        let chip_name = gtk::Label::new(Some(&choices[which.get()].name));
        chip_row.append(&chip_name);
        let chip = gtk::Button::builder()
            .child(&chip_row)
            .css_classes(["chip", "edge"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::End)
            .margin_start(28)
            .margin_bottom(30)
            .focus_on_click(false)
            .tooltip_text("Desktop session")
            .build();
        chip.update_property(&[gtk::accessible::Property::Label(&format!("Desktop session: {}", choices[which.get()].name))]);
        if choices.len() > 1 {
            let (choices, which, form_ref) = (choices.clone(), which.clone(), form.clone());
            chip.connect_clicked(move |chip| {
                let names: Vec<String> = choices.iter().map(|s| s.name.clone()).collect();
                let (choices, which, chip_name, chip_ref, form) = (choices.clone(), which.clone(), chip_name.clone(), chip.clone(), form_ref.clone());
                let popover = picker(chip, &names, which.get(), move |i| {
                    which.set(i);
                    chip_name.set_text(&choices[i].name);
                    chip_ref.update_property(&[gtk::accessible::Property::Label(&format!("Desktop session: {}", choices[i].name))]);
                    form.entry.grab_focus();
                });
                popover.set_position(gtk::PositionType::Top);
                popover.connect_closed(|popover| popover.unparent());
                popover.popup();
            });
        }
        if let Some(overlay) = form.battery.parent().and_downcast::<gtk::Overlay>() {
            overlay.add_overlay(&chip);
        }
        // three power buttons of 40 px with 10 px between, 28 px in from the edge, then a gap
        form.battery.set_halign(gtk::Align::End);
        form.battery.set_margin_start(0);
        form.battery.set_margin_end(28 + 3 * 40 + 2 * 10 + 18);
    }

    let submit = {
        let (app, form, people, choices, who, which) = (app.clone(), form.clone(), people.clone(), choices.clone(), who.clone(), which.clone());
        move || {
            let password = form.entry.text().to_string();
            if form.busy.get() || password.is_empty() {
                return;
            }
            let Some(person) = people.get(who.get()) else {
                say(&form, "There is no account to sign in to.", true);
                return;
            };
            let Some(session) = choices.get(which.get()).cloned() else {
                say(&form, "No desktop session is installed.", true);
                return;
            };
            form.busy.set(true);
            form.field.add_css_class("busy");
            form.entry.set_sensitive(false);
            form.go.set_sensitive(false);
            say(&form, "Signing in…", false);
            let login = person.login.clone();
            let env = session_env(&session);
            let check = {
                let (login, command) = (login.clone(), session.command.clone());
                gio::spawn_blocking(move || ask_greetd(&login, &password, &command, &env))
            };
            let (app, form) = (app.clone(), form.clone());
            glib::spawn_future_local(async move {
                let failure = match check.await {
                    Ok(Ok(())) => {
                        remember(&login, &session.file);
                        say(&form, "", false);
                        let app = app.clone();
                        after_exit(&form, move || app.quit());
                        return;
                    }
                    Ok(Err(failure)) => failure,
                    Err(_) => Failure::Broken("Signing in stopped unexpectedly.".into()),
                };
                form.busy.set(false);
                form.field.remove_css_class("busy");
                form.entry.set_sensitive(true);
                form.go.set_sensitive(true);
                form.entry.set_text("");
                form.entry.grab_focus();
                match failure {
                    Failure::Refused(said) => {
                        shake(&form);
                        say(&form, said.as_deref().unwrap_or("Wrong password. Try again."), true);
                    }
                    Failure::Broken(why) => say(&form, &format!("Could not sign in: {why}"), true),
                }
            });
        }
    };
    wire_form(&form, Rc::new(submit));

    // the clock, and the window kept alive by its timer
    let hold = app.hold();
    glib::timeout_add_seconds_local(1, move || {
        let _ = &hold;
        let (now_time, now_date) = (now("%H:%M"), now("%A, %-d %B"));
        if time.text() != now_time {
            time.set_text(&now_time);
        }
        if date.text() != now_date {
            date.set_text(&now_date);
        }
        glib::ControlFlow::Continue
    });
    {
        let (soft, frost, look) = (soft.clone(), form.frost.clone(), look.clone());
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            *soft.borrow_mut() = softened(look.get("wallpaper"));
            frost.queue_draw();
        });
    }
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_peoples_accounts_are_offered() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      arkasetup:x:1000:1000::/var/home/arkasetup:/bin/bash\n\
                      ram:x:1001:1001:Thulasi Ram,,,:/var/home/ram:/bin/bash\n\
                      ann:x:1002:1002::/var/home/ann:/bin/zsh\n\
                      svc:x:1003:1003::/:/sbin/nologin\n\
                      nobody:x:65534:65534:Kernel Overflow User:/:/sbin/nologin\n";
        let found = accounts(passwd);
        let logins: Vec<&str> = found.iter().map(|p| p.login.as_str()).collect();
        assert_eq!(logins, ["ann", "ram"]);
        assert_eq!(found[1].shown, "Thulasi Ram");
        assert_eq!(found[0].shown, "ann");
    }

    #[test]
    fn session_files_are_read() {
        let plasma = "[Desktop Entry]\nExec=/usr/libexec/plasma-dbus-run-session-if-needed /usr/bin/startplasma-wayland\nName=ArkaOS Desktop\nName[de]=Anders\nDesktopNames=KDE\n";
        let s = session("plasma.desktop", plasma).unwrap();
        assert_eq!(s.name, "ArkaOS Desktop");
        assert_eq!(s.command, ["/usr/libexec/plasma-dbus-run-session-if-needed", "/usr/bin/startplasma-wayland"]);
        assert_eq!(session_env(&s), ["XDG_SESSION_TYPE=wayland", "XDG_CURRENT_DESKTOP=KDE", "XDG_SESSION_DESKTOP=kde"]);
        assert_eq!(session("x.desktop", "[Desktop Entry]\nName=X\nExec=x\nHidden=true\n"), None);
        assert_eq!(session("x.desktop", "[Desktop Entry]\nName=X\n"), None);
    }

    #[test]
    fn the_last_sign_in_is_remembered() {
        let sddm = "[Last]\nUser=ram\nSession=/usr/share/wayland-sessions/arkawm.desktop\n";
        assert_eq!(remembered("", sddm), (Some("ram".into()), Some("arkawm.desktop".into())));
        let ours = "user = ann\nsession = plasma.desktop\n";
        assert_eq!(remembered(ours, sddm), (Some("ann".into()), Some("plasma.desktop".into())));
        assert_eq!(remembered("", ""), (None, None));
    }
}
