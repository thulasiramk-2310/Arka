//! One window per app. Every Arka app has a fixed application id, so launching it a second time
//! (dock, menu, tray, another app) re-activates the copy that is already running. Building a new
//! window on each activation opened a second window and a second background worker.
use gtk4::prelude::*;
use libadwaita as adw;

/// Run `build` the first time the app is activated; on later activations raise the existing window.
pub fn activate_once(app: &adw::Application, build: fn(&adw::Application)) {
    app.connect_activate(move |app| match app.windows().first() {
        Some(window) => window.present(),
        None => build(app),
    });
}
