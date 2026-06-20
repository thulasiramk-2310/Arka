use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

const APP_ID: &str = "org.arka.capsule";

#[derive(Clone)]
struct App {
    name:        &'static str,
    id:          &'static str,
    description: &'static str,
    icon:        &'static str,
    category:    &'static str,
}

const CATALOG: &[App] = &[
    App { name: "Signal",      id: "org.signal.Signal",            description: "Private, encrypted messenger",            icon: "chat-symbolic",                   category: "Privacy"      },
    App { name: "Bitwarden",   id: "com.bitwarden.desktop",        description: "Open-source password manager",            icon: "dialog-password-symbolic",        category: "Privacy"      },
    App { name: "KeePassXC",   id: "org.keepassxc.KeePassXC",      description: "Local password vault — no cloud",         icon: "dialog-password-symbolic",        category: "Privacy"      },
    App { name: "ProtonVPN",   id: "com.protonvpn.www",            description: "Privacy-first VPN",                       icon: "network-vpn-symbolic",            category: "Privacy"      },
    App { name: "VLC",         id: "org.videolan.VLC",             description: "Play any video or audio file",            icon: "multimedia-player-symbolic",      category: "Media"        },
    App { name: "Spotify",     id: "com.spotify.Client",           description: "Music streaming",                         icon: "audio-x-generic-symbolic",        category: "Media"        },
    App { name: "LibreOffice", id: "org.libreoffice.LibreOffice",  description: "Full office suite — free and open",       icon: "x-office-document-symbolic",      category: "Productivity" },
    App { name: "OnlyOffice",  id: "org.onlyoffice.desktopeditors",description: "Office suite compatible with .docx/.xlsx", icon: "x-office-spreadsheet-symbolic", category: "Productivity" },
    App { name: "GIMP",        id: "org.gimp.GIMP",                description: "Professional image editor",              icon: "image-x-generic-symbolic",        category: "Creative"     },
    App { name: "Inkscape",    id: "org.inkscape.Inkscape",        description: "Vector graphics editor",                  icon: "image-x-generic-symbolic",        category: "Creative"     },
    App { name: "OBS Studio",  id: "com.obsproject.Studio",        description: "Screen recording and streaming",          icon: "video-x-generic-symbolic",        category: "Creative"     },
    App { name: "Blender",     id: "org.blender.Blender",          description: "3D modelling and animation",              icon: "image-x-generic-symbolic",        category: "Creative"     },
    App { name: "Thunderbird", id: "org.mozilla.Thunderbird",      description: "Email client with encryption support",    icon: "mail-unread-symbolic",            category: "Productivity" },
    App { name: "VSCodium",    id: "com.vscodium.codium",          description: "Code editor — no Microsoft telemetry",    icon: "utilities-terminal-symbolic",     category: "Developer"    },
    App { name: "Bottles",     id: "com.usebottles.bottles",       description: "Run Windows apps in a container",         icon: "application-x-executable-symbolic", category: "Tools"    },
];

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    std::process::exit(app.run().value());
}

fn build_ui(app: &adw::Application) {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Capsule")
        .default_width(680)
        .default_height(620)
        .build();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data("
    .app-card { background-color: rgba(15,20,40,0.8); border-radius: 10px; border: 1px solid rgba(30,60,100,0.4); padding: 12px; }
    .app-name { font-size: 14px; font-weight: 700; color: #d8e8f8; }
    .app-desc { font-size: 11px; color: #3a5a78; }
    .cat-pill { background-color: rgba(30,60,100,0.4); border-radius: 12px; padding: 2px 10px; color: #4fc3f7; font-size: 11px; }
    .install-btn { }
    .installed-btn { color: #4ade80; }
    ");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let hb = adw::HeaderBar::new();
    hb.set_title_widget(Some(&adw::WindowTitle::new("Capsule", "App Store")));

    // Search
    let search = gtk4::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps…"));
    search.set_hexpand(true);

    let search_bar = gtk4::SearchBar::new();
    search_bar.set_child(Some(&search));
    search_bar.set_search_mode(true);
    search_bar.set_show_close_button(false);

    // Installed apps tracking (from flatpak list)
    let installed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(flatpak_installed()));

    // Content: flow box of app cards
    let flow = gtk4::FlowBox::new();
    flow.set_valign(gtk4::Align::Start);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.set_min_children_per_line(2);
    flow.set_max_children_per_line(3);
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_margin_start(16); flow.set_margin_end(16);
    flow.set_margin_top(8); flow.set_margin_bottom(16);

    let installed_ref = installed.clone();
    populate_flow(&flow, CATALOG, "", &installed_ref.borrow());

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&flow)
        .build();

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.append(&search_bar);
    body.append(&scroll);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&hb);
    toolbar.set_content(Some(&body));
    window.set_content(Some(&toolbar));

    // Search filter
    let flow_ref = flow.clone();
    let installed_ref2 = installed.clone();
    search.connect_search_changed(move |e| {
        let q = e.text().to_string();
        let inst = installed_ref2.borrow();
        populate_flow(&flow_ref, CATALOG, &q, &inst);
    });

    window.present();
}

fn populate_flow(flow: &gtk4::FlowBox, apps: &[App], query: &str, installed: &[String]) {
    while let Some(child) = flow.first_child() { flow.remove(&child); }

    let q = query.to_lowercase();
    let filtered: Vec<_> = apps.iter().filter(|a| {
        q.is_empty()
            || a.name.to_lowercase().contains(&q)
            || a.description.to_lowercase().contains(&q)
            || a.category.to_lowercase().contains(&q)
    }).collect();

    for app in filtered {
        let is_installed = installed.iter().any(|id| id == app.id);
        flow.insert(&make_card(app, is_installed), -1);
    }
}

fn make_card(app: &App, installed: bool) -> gtk4::FlowBoxChild {
    let child = gtk4::FlowBoxChild::new();
    child.set_focusable(false);

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    card.add_css_class("app-card");
    card.set_size_request(190, -1);

    // Top row: icon + category
    let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let icon = gtk4::Image::from_icon_name(app.icon);
    icon.set_pixel_size(36);
    top.append(&icon);

    let cat = gtk4::Label::new(Some(app.category));
    cat.add_css_class("cat-pill");
    cat.set_valign(gtk4::Align::Center);
    cat.set_hexpand(true);
    cat.set_halign(gtk4::Align::End);
    top.append(&cat);
    card.append(&top);

    // Name
    let name = gtk4::Label::new(Some(app.name));
    name.add_css_class("app-name");
    name.set_halign(gtk4::Align::Start);
    card.append(&name);

    // Description
    let desc = gtk4::Label::new(Some(app.description));
    desc.add_css_class("app-desc");
    desc.set_halign(gtk4::Align::Start);
    desc.set_wrap(true);
    desc.set_lines(2);
    card.append(&desc);

    // Install / Installed button
    let btn = if installed {
        let b = gtk4::Button::with_label("Installed ✓");
        b.add_css_class("installed-btn");
        b.set_sensitive(false);
        b
    } else {
        let b = gtk4::Button::with_label("Install");
        b.add_css_class("suggested-action");
        let id = app.id;
        b.connect_clicked(move |_| install_app(id));
        b
    };
    btn.set_halign(gtk4::Align::Fill);
    card.append(&btn);

    child.set_child(Some(&card));
    child
}

fn install_app(id: &str) {
    let cmd = format!(
        "flatpak install -y flathub '{}' && echo '✓ Installed!' || echo '✗ Installation failed'; read -p 'Press Enter to close...'",
        id
    );
    let _ = std::process::Command::new("foot")
        .args(["-e", "sh", "-c", &cmd])
        .spawn();
}

fn flatpak_installed() -> Vec<String> {
    let out = std::process::Command::new("flatpak")
        .args(["list", "--app", "--columns=application"])
        .output()
        .ok();
    let Some(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}
