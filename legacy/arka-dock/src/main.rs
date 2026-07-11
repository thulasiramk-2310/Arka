use arka_shell_common::theme;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Button, Image, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

const APP_ID: &str = "org.arka.dock";

const STYLE: &str = "
window { background: transparent; }

.dock-outer {
    padding: 8px;
}

.dock-pill {
    background-color: @bg_raised;
    border: 1px solid @border_ui;
    border-radius: 14px;
    padding: 7px 10px;
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
}

.dock-btn {
    background: transparent;
    border: none;
    border-radius: 8px;
    padding: 0;
    min-width: 34px;
    min-height: 34px;
    box-shadow: none;
    outline: none;
}
.dock-btn:hover {
    background-color: @bg_overlay;
}
.dock-btn image { color: @text_hi; }

/* Running indicator dot */
.dock-dot {
    font-size: 5px;
    color: @accent;
    margin-top: 2px;
}
.dock-dot-hidden { opacity: 0; }

.dock-logo-btn {
    background-color: @accent;
    border: none;
    border-radius: 8px;
    padding: 0;
    min-width: 34px;
    min-height: 34px;
    box-shadow: none;
}
.dock-logo-btn:hover { background-color: @accent_dim; }
.dock-logo-label { font-size: 16px; font-weight: 800; color: @bg_base; }

.dock-sep {
    background-color: @border_ui;
    margin: 4px 6px;
    min-width: 1px;
    min-height: 24px;
}
";

struct DockApp {
    icon: &'static str,
    tooltip: &'static str,
    cmd: &'static str,
    wm_class: &'static str,
}

const DOCK_APPS: &[DockApp] = &[
    DockApp { icon: "folder-symbolic",             tooltip: "Files",       cmd: "thunar",           wm_class: "thunar" },
    DockApp { icon: "utilities-terminal-symbolic", tooltip: "Terminal",    cmd: "foot",             wm_class: "foot" },
    DockApp { icon: "web-browser-symbolic",        tooltip: "Firefox",     cmd: "firefox",          wm_class: "firefox" },
    DockApp { icon: "mail-unread-symbolic",        tooltip: "Thunderbird", cmd: "thunderbird",      wm_class: "thunderbird" },
    DockApp { icon: "application-x-addon-symbolic",tooltip: "Capsule",     cmd: "arka-capsule",     wm_class: "org.arka.capsule" },
    DockApp { icon: "security-high-symbolic",      tooltip: "Privacy",     cmd: "arka-dashboard",   wm_class: "org.arka.dashboard" },
    DockApp { icon: "emblem-system-symbolic",      tooltip: "Settings",    cmd: "arka-settings-gtk",wm_class: "org.arka.settings" },
];

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("arka-dock")
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Bottom);
    window.set_anchor(Edge::Bottom, true);
    // Not anchoring left/right keeps it centered
    window.set_exclusive_zone(62); // dock height + margin

    theme::install_base();
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display connection"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let outer = GtkBox::new(Orientation::Horizontal, 0);
    outer.add_css_class("dock-outer");

    let pill = GtkBox::new(Orientation::Horizontal, 4);
    pill.add_css_class("dock-pill");

    // ── Launcher logo button ──────────────────────────────────────────────
    let logo_btn = Button::new();
    logo_btn.add_css_class("dock-logo-btn");
    let logo_lbl = Label::new(Some("A"));
    logo_lbl.add_css_class("dock-logo-label");
    logo_btn.set_child(Some(&logo_lbl));
    logo_btn.set_tooltip_text(Some("Arka Launcher"));
    logo_btn.connect_clicked(|_| { let _ = std::process::Command::new("arka-launcher").spawn(); });
    pill.append(&logo_btn);

    // Separator
    let sep1 = gtk4::Separator::new(Orientation::Vertical);
    sep1.add_css_class("dock-sep");
    pill.append(&sep1);

    // ── App buttons ───────────────────────────────────────────────────────
    let mut dot_labels: Vec<Label> = Vec::new();

    for app_def in DOCK_APPS {
        let item = GtkBox::new(Orientation::Vertical, 0);
        item.set_halign(gtk4::Align::Center);

        let btn = Button::new();
        btn.add_css_class("dock-btn");
        btn.set_tooltip_text(Some(app_def.tooltip));

        let icon = Image::from_icon_name(app_def.icon);
        icon.set_pixel_size(20);
        btn.set_child(Some(&icon));

        let cmd_owned = app_def.cmd.to_string();
        btn.connect_clicked(move |_| { let _ = std::process::Command::new(&cmd_owned).spawn(); });

        // Running indicator dot
        let dot = Label::new(Some("●"));
        dot.add_css_class("dock-dot");
        dot.add_css_class("dock-dot-hidden");
        dot.set_halign(gtk4::Align::Center);
        dot_labels.push(dot.clone());

        item.append(&btn);
        item.append(&dot);
        pill.append(&item);
    }

    outer.append(&pill);
    window.set_child(Some(&outer));

    // Poll running windows every 5s to update indicators
    let classes: Vec<&'static str> = DOCK_APPS.iter().map(|a| a.wm_class).collect();
    update_indicators(&dot_labels, &classes);
    glib::timeout_add_seconds_local(5, {
        let dot_labels = dot_labels.clone();
        move || {
            update_indicators(&dot_labels, &classes);
            glib::ControlFlow::Continue
        }
    });

    window.present();
}

fn update_indicators(dots: &[Label], classes: &[&str]) {
    let running = get_running_classes();
    for (dot, cls) in dots.iter().zip(classes.iter()) {
        let is_running = running.iter().any(|r| r.to_lowercase().contains(cls));
        if is_running {
            dot.remove_css_class("dock-dot-hidden");
        } else {
            dot.add_css_class("dock-dot-hidden");
        }
    }
}

fn get_running_classes() -> Vec<String> {
    let out = std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    // Extract "class" values from JSON array
    let mut classes = Vec::new();
    let mut rest = out.as_str();
    while let Some(pos) = rest.find("\"class\"") {
        rest = &rest[pos + 7..];
        let after = rest.trim_start_matches([' ', ':']);
        let after = after.trim_start_matches('"');
        if let Some(end) = after.find('"') {
            let cls = &after[..end];
            if !cls.is_empty() {
                classes.push(cls.to_string());
            }
            rest = &after[end..];
        }
    }
    classes
}
