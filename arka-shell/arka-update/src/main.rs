use adw::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;

mod operation;
use operation::Operation;

const APP_ID: &str = "org.arka.update";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    arka_shell_common::app::activate_once(&app, build_ui);
    std::process::exit(app.run().value());
}

const CSS: &str = "
.up-row { padding: 13px 2px; border-top: 1px solid alpha(@window_fg_color, 0.12); }
.up-rows { border-bottom: 1px solid alpha(@window_fg_color, 0.12); }
.up-key { font-size: 15px; font-weight: 600; }
.up-val { font-size: 14px; color: alpha(@window_fg_color, 0.75); }
.up-h { font-size: 17px; font-weight: 600; }
.up-note { font-size: 13px; color: alpha(@window_fg_color, 0.75); }
button.up-quiet { border-radius: 999px; padding: 8px 18px; }
";

fn build_ui(app: &adw::Application) {
    // Follow the desktop look (Light/Dark) instead of forcing dark.
    arka_shell_common::theme::follow_look();
    let css = gtk4::CssProvider::new();
    css.load_from_data(CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Updates")
        .default_width(620)
        .default_height(640)
        .build();

    let label = |t: &str, c: &str| {
        let l = gtk4::Label::new(Some(t));
        l.add_css_class(c);
        l.set_xalign(0.0);
        l.set_wrap(true);
        l
    };

    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    col.set_margin_top(28);
    col.set_margin_bottom(28);
    col.set_margin_start(32);
    col.set_margin_end(32);
    col.append(&label("Updates", "arka-title"));
    let sub = label(
        "A new version is prepared next to the one you're using and only takes over when you \
         restart. The version you have now is kept, so you can go back to it.",
        "arka-sub",
    );
    sub.set_margin_top(8);
    col.append(&sub);

    // status rows
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rows.add_css_class("up-rows");
    rows.set_margin_top(24);
    let mut vals = Vec::new();
    for k in ["Installed version", "Downloaded update", "Restart needed", "Previous version"] {
        let r = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        r.add_css_class("up-row");
        let key = label(k, "up-key");
        key.set_hexpand(true);
        let v = gtk4::Label::new(Some("Checking…"));
        v.add_css_class("up-val");
        v.set_wrap(true);
        v.set_xalign(1.0);
        r.append(&key);
        r.append(&v);
        rows.append(&r);
        vals.push(v);
    }
    col.append(&rows);

    // update actions
    let check_btn = gtk4::Button::with_label("Check for updates");
    check_btn.add_css_class("suggested-action");
    check_btn.add_css_class("up-quiet");
    let update_btn = gtk4::Button::with_label("Download update");
    update_btn.add_css_class("up-quiet");
    let restart_btn = gtk4::Button::with_label("Restart now");
    restart_btn.add_css_class("up-quiet");
    restart_btn.set_visible(false);
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    actions.set_margin_top(20);
    actions.append(&check_btn);
    actions.append(&update_btn);
    actions.append(&restart_btn);
    col.append(&actions);
    let act_note = label(
        "Checking and downloading open a terminal and ask for your password. Nothing restarts \
         until you choose to.",
        "up-note",
    );
    act_note.set_margin_top(10);
    col.append(&act_note);

    // going back
    let back_h = label("Go back to the previous version", "up-h");
    back_h.set_margin_top(32);
    col.append(&back_h);
    let back_note = label(
        "Use this if something stopped working after an update. It changes the system only: files \
         and settings in your home folder stay as they are now, and older copies of personal files \
         are not restored. Any downloaded update is discarded.",
        "up-note",
    );
    back_note.set_margin_top(8);
    col.append(&back_note);
    let rb_btn = gtk4::Button::with_label("Use the previous version after restart");
    rb_btn.add_css_class("up-quiet");
    rb_btn.set_halign(gtk4::Align::Start);
    rb_btn.set_margin_top(14);
    rb_btn.set_sensitive(false);
    col.append(&rb_btn);

    let status = label("", "up-note");
    status.set_margin_top(20);
    col.append(&status);

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&adw::Clamp::builder().maximum_size(640).child(&col).build())
        .build();
    let toolbar = adw::ToolbarView::new();
    let hb = adw::HeaderBar::new();
    hb.set_show_title(false);
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&scroll));
    window.set_content(Some(&toolbar));

    let refresh: std::rc::Rc<dyn Fn()> = {
        let (vals, restart_btn, rb_btn) = (vals.clone(), restart_btn.clone(), rb_btn.clone());
        std::rc::Rc::new(move || {
            let (vals, restart_btn, rb_btn) = (vals.clone(), restart_btn.clone(), rb_btn.clone());
            gtk4::glib::spawn_future_local(async move {
                let res = gtk4::gio::spawn_blocking(read_status).await.unwrap_or_else(|_| Err("status reader crashed".into()));
                show_status(res, &vals, &restart_btn, &rb_btn);
            });
        })
    };
    refresh();
    // Also observe background updates and expire stale publications while open.
    let periodic_refresh = refresh.clone();
    let weak_window = window.downgrade();
    gtk4::glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
        if weak_window.upgrade().is_none() { return gtk4::glib::ControlFlow::Break; }
        periodic_refresh();
        gtk4::glib::ControlFlow::Continue
    });

    for (button, operation) in [
        (check_btn, Operation::Check),
        (update_btn, Operation::Upgrade),
        (rb_btn.clone(), Operation::Rollback),
    ] {
        let (status, refresh) = (status.clone(), refresh.clone());
        button.connect_clicked(move |_| run_in_terminal(operation, &status, refresh.clone()));
    }
    restart_btn.connect_clicked(|_| {
        let _ = std::process::Command::new("systemctl").arg("reboot").spawn();
    });

    window.present();
    arka_shell_common::theme::preview_shot(&window);
}

/// Published by arka-bootc-status.service; reading needs no privileges.
fn read_status() -> Result<arka_update::status::UpdateStatus, String> {
    arka_update::status::read_published()
}

fn show_status(
    res: Result<arka_update::status::UpdateStatus, String>,
    vals: &[gtk4::Label],
    restart_btn: &gtk4::Button,
    rb_btn: &gtk4::Button,
) {
    let ver = |d: &arka_update::status::Deployment| {
        d.version.clone().or_else(|| d.image.clone()).unwrap_or_else(|| "Unknown".into())
    };
    match res {
        Ok(u) => {
            // ArkaOS's own version first; the image's version label is Fedora's base.
            vals[0].set_text(&format!("{} (Fedora base {})", current_version(), ver(&u.installed)));
            vals[1].set_text(&match &u.staged {
                Some(d) if d.download_only => format!("{} (downloaded, not yet applied)", ver(d)),
                Some(d) => format!("{} (ready)", ver(d)),
                None => "None".into(),
            });
            vals[2].set_text(if u.restart_needed { "Yes" } else { "No" });
            vals[3].set_text(if u.rollback_queued {
                "Selected for the next restart"
            } else if u.rollback_available {
                "Available"
            } else {
                "None on this computer yet"
            });
            restart_btn.set_visible(u.restart_needed);
            rb_btn.set_sensitive(u.rollback_available && !u.rollback_queued);
        }
        Err(e) => {
            vals[0].set_text(&current_version());
            for v in &vals[1..] { v.set_text("Couldn't check"); }
            vals[1].set_tooltip_text(Some(&e));
            restart_btn.set_visible(false);
            rb_btn.set_sensitive(false);
        }
    }
}

fn current_version() -> String {
    let v = std::fs::read_to_string("/usr/lib/arkaos-release").unwrap_or_default();
    let v = v.trim();
    if v.is_empty() {
        "Unknown".into()
    } else {
        v.into()
    }
}

fn run_in_terminal(operation: Operation, status: &gtk4::Label, refresh: std::rc::Rc<dyn Fn()>) {
    match std::process::Command::new("konsole")
        .args(["--nofork", "-e", "sh", "-c", &operation.script()])
        .spawn()
    {
        Ok(mut child) => {
            status.set_label("Follow the progress and the result in the terminal window.");
            let status = status.clone();
            gtk4::glib::spawn_future_local(async move {
                let _ = gtk4::gio::spawn_blocking(move || child.wait()).await;
                status.set_label("");
                // arka-bootc-status republishes when deployments change; give it a moment.
                gtk4::glib::timeout_future(std::time::Duration::from_secs(3)).await;
                refresh();
            });
        }
        Err(error) => status.set_label(&format!("Could not open the update terminal: {error}")),
    }
}
