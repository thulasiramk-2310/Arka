use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::process::Command;

const APP_ID: &str = "org.arka.perms";

mod permissions;
use permissions::{is_granted, Perm, PERMS};

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("App Permissions")
        .default_width(620)
        .default_height(680)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .perm-app-name { font-size: 15px; font-weight: 700; color: #d8e8f8; }
    .perm-app-id   { font-size: 11px; color: #3a5a78; }
    .danger-row    { color: #f28b35; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("App Permissions", "ArkaOS")));

    // Main content stack: app list → permission detail
    let nav = adw::NavigationView::new();
    nav.set_animate_transitions(true);

    // ── Page 1: installed app list ──────────────────────────────────────────
    let app_list_page = adw::NavigationPage::new(
        &build_app_list_page(&nav),
        "Apps",
    );
    nav.push(&app_list_page);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&nav));
    window.set_content(Some(&toolbar));

    // Esc closes
    let ctl = gtk4::EventControllerKey::new();
    let w = window.clone();
    ctl.connect_key_pressed(move |_, key, _, _| {
        if key == gtk4::gdk::Key::Escape { w.close(); glib::Propagation::Stop }
        else { glib::Propagation::Proceed }
    });
    window.add_controller(ctl);

    window.present();
}

fn build_app_list_page(nav: &adw::NavigationView) -> gtk4::Widget {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let target = content.clone();
    let nav = nav.clone();
    glib::spawn_future_local(async move {
    let apps = match gtk4::gio::spawn_blocking(list_flatpak_apps).await {
        Ok(Ok(apps)) => apps,
        _ => {
            target.append(&gtk4::Label::new(Some("Could not read installed apps. Check that Flatpak is available.")));
            return;
        }
    };
    let content = target;
    if apps.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_icon_name(Some("application-x-executable-symbolic"));
        empty.set_title("No Sandboxed Apps");
        empty.set_description(Some("Install apps via Capsule to manage their permissions here."));
        content.append(&empty);
        return;
    }

    let group = adw::PreferencesGroup::new();
    group.set_title("Installed Apps");
    group.set_description(Some("Tap an app to review and change what it can access."));
    group.set_margin_start(16);
    group.set_margin_end(16);
    group.set_margin_top(16);
    group.set_margin_bottom(16);

    for (id, name) in &apps {
        let row = adw::ActionRow::new();
        row.set_title(name);
        row.set_subtitle(id);
        row.set_activatable(true);

        let icon = gtk4::Image::from_icon_name("application-x-executable-symbolic");
        icon.set_pixel_size(32);
        row.add_prefix(&icon);

        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        row.add_suffix(&chevron);

        let nav_ref = nav.clone();
        let id_owned = id.clone();
        let name_owned = name.clone();
        row.connect_activated(move |_| {
            let perm_page = build_perm_page(&id_owned, &name_owned);
            let nav_page = adw::NavigationPage::new(&perm_page, &name_owned);
            nav_ref.push(&nav_page);
        });

        group.add(&row);
    }

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    inner.append(&group);
    scroll.set_child(Some(&inner));
    content.append(&scroll);
    });
    content.upcast()
}

fn build_perm_page(app_id: &str, app_name: &str) -> gtk4::Widget {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let target = content.clone();
    let app_id = app_id.to_owned();
    let app_name = app_name.to_owned();
    glib::spawn_future_local(async move {
    let aid = app_id.clone();
    let current_perms = match gtk4::gio::spawn_blocking(move || read_app_perms(&aid)).await {
        Ok(Ok(perms)) => perms,
        _ => {
            target.append(&gtk4::Label::new(Some("Permissions unavailable. No settings were changed.")));
            return;
        }
    };
    let content = target;
    // App header card
    let header_group = adw::PreferencesGroup::new();
    header_group.set_margin_start(16);
    header_group.set_margin_end(16);
    header_group.set_margin_top(16);
    header_group.set_margin_bottom(4);

    let header_row = adw::ActionRow::new();
    header_row.set_title(&app_name);
    header_row.set_subtitle(&app_id);
    let app_icon = gtk4::Image::from_icon_name("application-x-executable-symbolic");
    app_icon.set_pixel_size(40);
    header_row.add_prefix(&app_icon);
    header_group.add(&header_row);

    // Reset button
    let reset_btn = gtk4::Button::with_label("Reset to Defaults");
    reset_btn.add_css_class("destructive-action");
    reset_btn.add_css_class("pill");
    reset_btn.set_halign(gtk4::Align::Center);
    reset_btn.set_margin_top(8);
    reset_btn.set_margin_bottom(4);
    let notice = gtk4::Label::new(Some("Changes apply when the app is restarted. Portals and broader grants can provide additional access."));
    notice.set_wrap(true);
    notice.set_margin_start(16);
    notice.set_margin_end(16);
    let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(adw::SwitchRow, &'static Perm)>::new()));
    let updating = std::rc::Rc::new(std::cell::Cell::new(false));
    let aid = app_id.to_string();
    let all_rows = rows.clone();
    let busy = updating.clone();
    let note = notice.clone();
    reset_btn.connect_clicked(move |_| {
        change_permissions(aid.clone(), None, all_rows.clone(), busy.clone(), note.clone());
    });

    // Permissions group
    let perm_group = adw::PreferencesGroup::new();
    perm_group.set_title("Direct permission grants");
    perm_group.set_margin_start(16);
    perm_group.set_margin_end(16);
    perm_group.set_margin_top(8);
    perm_group.set_margin_bottom(8);

    for perm in PERMS {
        let row = adw::SwitchRow::new();
        row.set_title(perm.label);
        row.set_subtitle(perm.desc);

        let prefix_icon = gtk4::Image::from_icon_name(perm.icon);
        prefix_icon.set_pixel_size(20);
        row.add_prefix(&prefix_icon);

        row.set_active(is_granted(perm, &current_perms));
        rows.borrow_mut().push((row.clone(), perm));
        let aid = app_id.to_string();
        let all_rows = rows.clone();
        let busy = updating.clone();
        let note = notice.clone();
        row.connect_active_notify(move |r| {
            if busy.get() { return; }
            let flag = if r.is_active() { perm.allow } else { perm.deny };
            change_permissions(aid.clone(), Some(flag), all_rows.clone(), busy.clone(), note.clone());
        });

        perm_group.add(&row);
    }

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    inner.append(&header_group);
    inner.append(&perm_group);
    inner.append(&reset_btn);
    inner.append(&notice);
    scroll.set_child(Some(&inner));
    content.append(&scroll);
    });
    content.upcast()
}

type Rows = std::rc::Rc<std::cell::RefCell<Vec<(adw::SwitchRow, &'static Perm)>>>;

fn change_permissions(aid: String, flag: Option<&'static str>, rows: Rows,
    busy: std::rc::Rc<std::cell::Cell<bool>>, note: gtk4::Label) {
    if busy.replace(true) { return; }
    for (row, _) in rows.borrow().iter() { row.set_sensitive(false); }
    glib::spawn_future_local(async move {
        let result = gtk4::gio::spawn_blocking(move || {
            let applied = flatpak(&["override", "--user", flag.unwrap_or("--reset"), &aid]);
            (applied, read_app_perms(&aid))
        }).await;
        match result {
            Ok((applied, Ok(perms))) => {
                for (row, perm) in rows.borrow().iter() {
                    row.set_active(is_granted(perm, &perms));
                    row.set_sensitive(true);
                }
                note.set_text(if applied.is_ok() { "Saved. Restart the app to apply changes." }
                    else { "Change failed. The switches show the settings currently reported by Flatpak." });
            }
            _ => note.set_text("Could not verify permissions. Reopen this page to retry."),
        }
        busy.set(false);
    });
}

fn flatpak(args: &[&str]) -> Result<String, String> {
    let out = Command::new("timeout")
        .args(["--kill-after=2", "15", "flatpak"]).args(args)
        .env("LC_ALL", "C").output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(format!("Flatpak failed: {}", out.status)); }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

fn list_flatpak_apps() -> Result<Vec<(String, String)>, String> {
    Ok(flatpak(&["list", "--app", "--columns=application,name"])?
        .lines().filter_map(|line| {
            let (id, name) = line.split_once('\t').unwrap_or((line, line));
            (!id.is_empty()).then(|| (id.to_owned(), name.to_owned()))
        }).collect())
}

fn read_app_perms(app_id: &str) -> Result<String, String> {
    // Flatpak merges the manifest with system and user overrides here.
    flatpak(&["info", "--show-permissions", app_id])
}
