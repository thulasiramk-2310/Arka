use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

const APP_ID: &str = "org.arka.bar";

const STYLE: &str = "
.arka-bar { background-color: #0f0f14; padding: 2px 12px; }
.brand { color: #4fc3f7; font-weight: bold; letter-spacing: 2px; }
.privacy-score { color: #8bd17c; margin-right: 16px; }
.clock { color: #cfcfcf; }
";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("arka-bar")
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.auto_exclusive_zone_enable();
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);

    let root = GtkBox::new(Orientation::Horizontal, 0);
    root.add_css_class("arka-bar");

    let brand = Label::new(Some("ARKA"));
    brand.add_css_class("brand");

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let privacy_label = Label::new(Some("Privacy --"));
    privacy_label.add_css_class("privacy-score");

    let clock_label = Label::new(Some(""));
    clock_label.add_css_class("clock");

    root.append(&brand);
    root.append(&spacer);
    root.append(&privacy_label);
    root.append(&clock_label);

    window.set_child(Some(&root));

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display connection"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    glib::timeout_add_seconds_local(1, {
        let clock_label = clock_label.clone();
        move || {
            clock_label.set_text(&chrono::Local::now().format("%H:%M  %Y-%m-%d").to_string());
            glib::ControlFlow::Continue
        }
    });

    update_privacy_score(&privacy_label);
    glib::timeout_add_seconds_local(5, {
        let privacy_label = privacy_label.clone();
        move || {
            update_privacy_score(&privacy_label);
            glib::ControlFlow::Continue
        }
    });

    window.present();
}

fn update_privacy_score(label: &Label) {
    match fetch_privacy_score() {
        Ok(score) => label.set_text(&format!("Privacy {score}")),
        Err(_) => label.set_text("Privacy --"),
    }
}

fn fetch_privacy_score() -> zbus::Result<u32> {
    let conn = zbus::blocking::Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.arka.arkad",
        "/org/arka/arkad",
        "org.arka.arkad",
    )?;
    proxy.get_property::<u32>("PrivacyScore")
}
