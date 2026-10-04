mod dbus;
mod state;
mod ui;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use libadwaita as adw;
use adw::prelude::*;

const APP_ID: &str = "org.arka.dashboard";

fn main() {
    tracing_subscriber::fmt().without_time().with_target(false).init();
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();
    app.connect_activate(build_window);
    std::process::exit(app.run().value());
}

fn build_window(app: &adw::Application) {
    // Follow the desktop look (Light/Dark) instead of forcing dark.
    arka_shell_common::theme::follow_look();

    let (tx, rx) = std::sync::mpsc::channel::<state::StateUpdate>();
    let rx = Arc::new(Mutex::new(rx));

    dbus::start_worker(tx.clone());

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Privacy Dashboard")
        .default_width(980)
        .default_height(680)
        .build();

    let updater = ui::build(&window, tx);

    let rx_ref = rx.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(update) = rx_ref.lock().unwrap().try_recv() {
            updater(update);
        }
        glib::ControlFlow::Continue
    });

    window.present();
    arka_shell_common::theme::preview_shot(&window);
}
