use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

const APP_ID: &str = "org.arka.update";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("ArkaOS Updates")
        .default_width(440)
        .default_height(380)
        .resizable(false)
        .build();

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("System Updates", "ArkaOS")));

    // Current version
    let ver_group = adw::PreferencesGroup::new();
    ver_group.set_title("Installed");
    let ver_row = adw::ActionRow::new();
    ver_row.set_title("ArkaOS");
    ver_row.set_subtitle(&current_version());
    ver_row.add_prefix(&gtk4::Image::from_icon_name("system-software-install-symbolic"));
    ver_group.add(&ver_row);

    // Update section
    let upd_group = adw::PreferencesGroup::new();
    upd_group.set_title("Over-the-Air Update");
    upd_group.set_description(Some("Updates are atomic and signed. Your data is never touched."));

    let check_btn = gtk4::Button::with_label("Check for Updates");
    check_btn.add_css_class("suggested-action");
    check_btn.add_css_class("pill");

    let update_btn = gtk4::Button::with_label("Update Now");
    update_btn.add_css_class("pill");

    let btns = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    btns.set_halign(gtk4::Align::Center);
    btns.set_margin_top(8);
    btns.set_margin_bottom(8);
    btns.append(&check_btn);
    btns.append(&update_btn);
    upd_group.add(&btns);

    // Rollback section
    let rb_group = adw::PreferencesGroup::new();
    rb_group.set_title("Recovery");
    rb_group.set_description(Some("Roll back to the previous working version of ArkaOS."));

    let rb_btn = gtk4::Button::with_label("Roll Back to Previous Version");
    rb_btn.add_css_class("destructive-action");
    rb_btn.add_css_class("pill");
    let rb_wrap = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    rb_wrap.set_halign(gtk4::Align::Center);
    rb_wrap.set_margin_top(8);
    rb_wrap.set_margin_bottom(8);
    rb_wrap.append(&rb_btn);
    rb_group.add(&rb_wrap);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.append(&ver_group);
    content.append(&upd_group);
    content.append(&rb_group);

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&content)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&scroll));
    window.set_content(Some(&toolbar));

    // Handlers — open foot terminal for long-running commands
    check_btn.connect_clicked(|_| run_in_terminal("sudo bootc upgrade --check"));
    update_btn.connect_clicked(|_| {
        run_in_terminal("sudo bootc upgrade && echo '✓ Update complete. Reboot to apply.' && read -p 'Press Enter to close...'");
    });
    rb_btn.connect_clicked(|_| {
        run_in_terminal("sudo bootc rollback && echo '✓ Rolled back. Reboot to apply.' && read -p 'Press Enter to close...'");
    });

    window.present();
}

fn current_version() -> String {
    let v = std::fs::read_to_string("/etc/arkaos-release")
        .unwrap_or_default();
    let v = v.trim();
    if v.is_empty() { "0.1".into() } else { v.into() }
}

fn run_in_terminal(cmd: &str) {
    let _ = std::process::Command::new("konsole")
        .args(["-e", "sh", "-c", cmd])
        .spawn();
}
