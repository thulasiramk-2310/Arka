//! App Library: find and install apps from Flathub, after seeing what each one can reach.
//! Everything shown comes from Flathub's own catalogue and from `flatpak` itself; installs
//! go through `arka install`, for the user's account only.

mod catalog;
mod perms;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;

use catalog::{App, Para, Pick};

// The id stays the one Capsule had, so docks and menus that pinned it keep working.
const APP_ID: &str = "org.arka.capsule";
const PAGE: usize = 60;
const REMOTE_URL: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

/// Sidebar sections: title, icon, freedesktop categories it shows.
const SECTIONS: &[(&str, &str, &[&str])] = &[
    ("Explore", "compass", &[]),
    ("Work & office", "briefcase", &["Office"]),
    ("Internet", "globe", &["Network", "WebBrowser", "Email"]),
    ("Messaging", "chat-circle", &["InstantMessaging", "Chat"]),
    ("Audio & video", "film-strip", &["AudioVideo", "Audio", "Video"]),
    ("Graphics", "palette", &["Graphics"]),
    ("Development", "code", &["Development"]),
    ("Games", "game-controller", &["Game"]),
];
const INSTALLED: usize = 100;

const CSS: &str = "
.al-card { padding: 16px; border-radius: 12px; }
.al-card:hover { background-color: alpha(@card_fg_color, 0.04); }
.al-card label { color: @card_fg_color; }
.al-card label.dim-label { color: alpha(@card_fg_color, 0.62); }
.al-name { font-weight: 600; font-size: 15px; }
.al-dev { font-size: 13px; }
.al-sum { font-size: 13.5px; }
.al-verified { color: @accent; }
.al-h1 { font-size: 26px; font-weight: 600; }
.al-h2 { font-size: 15px; font-weight: 600; }
.al-sub { font-size: 14px; }
.al-fact-k { font-size: 12px; }
.al-fact-v { font-size: 15px; font-weight: 600; }
.al-wide { color: @warn; font-weight: 600; font-size: 12px; }
.al-badge { background: @text_hi; color: @bg_base; border-radius: 999px; padding: 0 7px; font-size: 11.5px; font-weight: 600; }
.al-err { color: @danger; }
.al-reach-row { padding: 10px 0; border-top: 1px solid @border_ui; }
";

#[derive(Clone)]
struct Inst {
    name: String,
    version: String,
    user: bool,
}

#[derive(Clone)]
struct Upd {
    id: String,
    version: String,
    size: String,
}

enum Load {
    Loading,
    Ready,
    Failed(String),
}

struct St {
    window: adw::ApplicationWindow,
    split: adw::NavigationSplitView,
    nav: adw::NavigationView,
    top_list: gtk4::ListBox,
    bottom_list: gtk4::ListBox,
    badge: gtk4::Label,
    search: gtk4::SearchEntry,
    apps: RefCell<Rc<Vec<App>>>,
    picks: Vec<Pick>,
    scope: Cell<&'static str>,
    installed: RefCell<HashMap<String, Inst>>,
    updates: RefCell<Vec<Upd>>,
    checked: RefCell<String>,
    /// Why the last "Update all" failed; shown on the Installed page until the next try.
    update_error: RefCell<Option<String>>,
    load: RefCell<Load>,
    current: Cell<usize>,
}

type S = Rc<St>;

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    arka_shell_common::app::activate_once(&app, build_ui);
    std::process::exit(app.run().value());
}

fn icon_dirs() -> Vec<PathBuf> {
    let mut d = vec![PathBuf::from("/usr/share/arka-capsule/icons")];
    if let Some(dev) = std::env::var_os("ARKA_APPLIB_ICONS") {
        d.push(PathBuf::from(dev));
    }
    if cfg!(debug_assertions) {
        d.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/icons")));
    }
    d
}

fn ic(name: &str) -> gtk4::Image {
    let i = gtk4::Image::from_icon_name(&format!("applib-{name}-symbolic"));
    i.set_pixel_size(16);
    i
}

fn label(text: &str, classes: &[&str]) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn build_ui(app: &adw::Application) {
    arka_shell_common::theme::follow_look();
    if let Some(display) = gtk4::gdk::Display::default() {
        let theme = gtk4::IconTheme::for_display(&display);
        for d in icon_dirs() {
            theme.add_search_path(d);
        }
        let p = gtk4::CssProvider::new();
        p.load_from_data(CSS);
        gtk4::style_context_add_provider_for_display(&display, &p, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("App Library")
        .default_width(1240)
        .default_height(820)
        .build();

    // Sidebar
    let top_list = gtk4::ListBox::new();
    top_list.add_css_class("navigation-sidebar");
    for (title, icon, _) in SECTIONS {
        top_list.append(&nav_row(icon, title, None));
    }
    let badge = gtk4::Label::new(None);
    badge.add_css_class("al-badge");
    badge.set_visible(false);
    let bottom_list = gtk4::ListBox::new();
    bottom_list.add_css_class("navigation-sidebar");
    bottom_list.append(&nav_row("download-simple", "Installed & updates", Some(&badge)));
    bottom_list.append(&nav_row("info", "About", None));
    let side_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    side_box.append(&top_list);
    let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    side_box.append(&spacer);
    side_box.append(&bottom_list);
    side_box.set_margin_bottom(8);
    let side_tb = adw::ToolbarView::new();
    side_tb.add_top_bar(&adw::HeaderBar::builder().title_widget(&adw::WindowTitle::new("App Library", "")).build());
    side_tb.set_content(Some(&side_box));
    let side_page = adw::NavigationPage::new(&side_tb, "App Library");

    // Content
    let search = gtk4::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps"));
    search.set_width_request(420);
    let nav = adw::NavigationView::new();
    let content_tb = adw::ToolbarView::new();
    content_tb.add_top_bar(&adw::HeaderBar::builder().title_widget(&search).show_back_button(true).build());
    content_tb.set_content(Some(&nav));
    let content_page = adw::NavigationPage::new(&content_tb, "Apps");

    let split = adw::NavigationSplitView::new();
    split.set_sidebar(Some(&side_page));
    split.set_content(Some(&content_page));
    split.set_min_sidebar_width(220.0);
    split.set_max_sidebar_width(260.0);
    window.set_content(Some(&split));
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 760sp").unwrap());
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);

    let st: S = Rc::new(St {
        window: window.clone(),
        split,
        nav,
        top_list: top_list.clone(),
        bottom_list: bottom_list.clone(),
        badge,
        search: search.clone(),
        apps: RefCell::new(Rc::new(Vec::new())),
        picks: catalog::load_picks(),
        scope: Cell::new("--user"),
        installed: RefCell::new(HashMap::new()),
        updates: RefCell::new(Vec::new()),
        checked: RefCell::new(String::new()),
        update_error: RefCell::new(None),
        load: RefCell::new(Load::Loading),
        current: Cell::new(0),
    });

    {
        let st = st.clone();
        top_list.connect_row_activated(move |_, row| {
            st.bottom_list.unselect_all();
            st.search.set_text("");
            show(&st, row.index() as usize);
        });
    }
    {
        let st = st.clone();
        bottom_list.connect_row_activated(move |list, row| {
            if row.index() == 1 {
                list.unselect_all();
                if st.current.get() == INSTALLED {
                    list.select_row(list.row_at_index(0).as_ref());
                }
                about(&st);
                return;
            }
            st.top_list.unselect_all();
            st.search.set_text("");
            show(&st, INSTALLED);
        });
    }
    {
        let st = st.clone();
        search.connect_search_changed(move |e| {
            let q = e.text().trim().to_string();
            if q.is_empty() {
                show(&st, st.current.get());
            } else {
                results(&st, &q);
            }
        });
    }
    let esc = gtk4::EventControllerKey::new();
    {
        let st = st.clone();
        esc.connect_key_pressed(move |_, k, _, m| {
            if k == gtk4::gdk::Key::f && m.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
                st.search.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    window.add_controller(esc);

    top_list.select_row(top_list.row_at_index(0).as_ref());
    show(&st, 0);
    window.present();
    arka_shell_common::theme::preview_shot(&window);

    let st2 = st.clone();
    glib::spawn_future_local(async move { start(&st2).await });
}

fn nav_row(icon: &str, title: &str, badge: Option<&gtk4::Label>) -> gtk4::ListBoxRow {
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    b.set_margin_top(4);
    b.set_margin_bottom(4);
    b.append(&ic(icon));
    let l = gtk4::Label::new(Some(title));
    l.set_xalign(0.0);
    l.set_hexpand(true);
    b.append(&l);
    if let Some(badge) = badge {
        b.append(badge);
    }
    let r = gtk4::ListBoxRow::new();
    r.set_child(Some(&b));
    r
}

// ── Loading ──────────────────────────────────────────────────────────────────

async fn run(args: &[&str]) -> (bool, String, String) {
    let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE);
    launcher.setenv("LC_ALL", "C", true);
    let argv: Vec<&std::ffi::OsStr> = args.iter().map(|s| std::ffi::OsStr::new(*s)).collect();
    match launcher.spawn(&argv) {
        Ok(p) => match p.communicate_utf8_future(None).await {
            Ok((o, e)) => (
                p.is_successful(),
                o.map(|s| s.to_string()).unwrap_or_default(),
                e.map(|s| s.to_string()).unwrap_or_default(),
            ),
            Err(e) => (false, String::new(), e.to_string()),
        },
        Err(e) => (false, String::new(), e.to_string()),
    }
}

fn last_line(s: &str) -> String {
    let l = s.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("unknown error");
    // flatpak starts its messages with "error: "; the label already says what failed.
    l.strip_prefix("error: ").or_else(|| l.strip_prefix("Error: ")).unwrap_or(l).to_string()
}

fn offline() -> bool {
    std::env::var_os("ARKA_APPLIB_OFFLINE").is_some()
}

async fn start(st: &S) {
    // Get or refresh the user's copy of Flathub's list (at most once a day).
    if !offline() {
        let file = catalog::user_dir().join("appstream.xml");
        let stale = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .map(|t| t.elapsed().map(|d| d.as_secs() > 86400).unwrap_or(true))
            .unwrap_or(true);
        if stale {
            let (ok, _, err) = run(&["flatpak", "remote-add", "--user", "--if-not-exists", "flathub", REMOTE_URL]).await;
            let (ok2, _, err2) = if ok {
                run(&["flatpak", "update", "--user", "--appstream", "flathub"]).await
            } else {
                (false, String::new(), err)
            };
            if !ok2 && catalog::locate().is_none() {
                *st.load.borrow_mut() = Load::Failed(last_line(&err2));
                refresh_page(st);
                return;
            }
        }
    }
    let Some((dir, scope)) = catalog::locate() else {
        *st.load.borrow_mut() = Load::Failed("Flathub's list isn't on this computer yet.".into());
        refresh_page(st);
        return;
    };
    st.scope.set(scope);
    match gio::spawn_blocking(move || catalog::load(&dir)).await {
        Ok(Ok(apps)) => {
            *st.apps.borrow_mut() = Rc::new(apps);
            *st.load.borrow_mut() = Load::Ready;
        }
        Ok(Err(e)) => *st.load.borrow_mut() = Load::Failed(e),
        Err(_) => *st.load.borrow_mut() = Load::Failed("Reading Flathub's list failed.".into()),
    }
    refresh_installed(st).await;
    open_requested(st);
    refresh_page(st);
    check_updates(st).await;
}

/// ARKA_APPLIB_PAGE=app:<id> | section:<n> | installed | search:<words> opens that page (previews, tests).
fn open_requested(st: &S) {
    let Ok(p) = std::env::var("ARKA_APPLIB_PAGE") else { return };
    if let Some(id) = p.strip_prefix("app:") {
        if let Some(a) = st.apps.borrow().iter().find(|a| a.id == id).cloned() {
            st.current.set(usize::MAX);
            st.nav.replace(&[]);
            show_section_page(st, 0);
            app_page(st, &a);
        }
    } else if let Some(n) = p.strip_prefix("section:").and_then(|n| n.parse().ok()) {
        st.top_list.select_row(st.top_list.row_at_index(n as i32).as_ref());
        st.current.set(n);
    } else if p == "installed" {
        st.top_list.unselect_all();
        st.bottom_list.select_row(st.bottom_list.row_at_index(0).as_ref());
        st.current.set(INSTALLED);
    } else if let Some(q) = p.strip_prefix("search:") {
        st.search.set_text(q);
    }
}

async fn refresh_installed(st: &S) {
    let (ok, out, _) = run(&["flatpak", "list", "--app", "--columns=application,name,version,installation"]).await;
    let mut m = HashMap::new();
    if ok {
        for line in out.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() >= 4 {
                m.insert(f[0].to_string(), Inst { name: f[1].into(), version: f[2].into(), user: f[3] == "user" });
            }
        }
    }
    *st.installed.borrow_mut() = m;
}

async fn check_updates(st: &S) {
    let (ok, out, _) = run(&["flatpak", "remote-ls", "--user", "--updates", "--app", "--columns=application,version,download-size"]).await;
    let mut v = Vec::new();
    if ok {
        for line in out.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() >= 3 && !f[0].is_empty() {
                v.push(Upd { id: f[0].into(), version: f[1].into(), size: f[2].trim().into() });
            }
        }
    }
    let n = v.len();
    *st.updates.borrow_mut() = v;
    *st.checked.borrow_mut() = if ok {
        glib::DateTime::now_local().ok().and_then(|t| t.format("%H:%M").ok()).map(|t| format!("Checked at {t}.")).unwrap_or_default()
    } else {
        "Couldn't check for updates. Check the internet connection and try again.".into()
    };
    st.badge.set_label(&n.to_string());
    st.badge.set_visible(n > 0);
    if st.current.get() == INSTALLED && st.search.text().is_empty() && st.nav.navigation_stack().n_items() <= 1 {
        show(st, INSTALLED);
    }
}

fn refresh_page(st: &S) {
    if !st.search.text().is_empty() {
        results(st, st.search.text().trim());
    } else {
        // Keep an open app page; rebuild the list under it.
        let depth = st.nav.navigation_stack().n_items();
        if depth <= 1 {
            show(st, st.current.get());
        }
    }
}

// ── Pages ────────────────────────────────────────────────────────────────────

fn scaffold(title: &str, sub: &str) -> (gtk4::ScrolledWindow, gtk4::Box) {
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.set_margin_top(28);
    body.set_margin_bottom(40);
    body.set_margin_start(36);
    body.set_margin_end(36);
    body.append(&label(title, &["al-h1"]));
    if !sub.is_empty() {
        let s = label(sub, &["al-sub", "dim-label"]);
        s.set_margin_top(6);
        body.append(&s);
    }
    let clamp = adw::Clamp::builder().maximum_size(1120).tightening_threshold(900).child(&body).build();
    let sc = gtk4::ScrolledWindow::new();
    sc.set_hscrollbar_policy(gtk4::PolicyType::Never);
    sc.set_child(Some(&clamp));
    (sc, body)
}

fn heading(body: &gtk4::Box, title: &str, note: &str) {
    let h = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    h.set_margin_top(30);
    h.set_margin_bottom(14);
    let t = label(title, &["al-h2"]);
    t.set_hexpand(true);
    h.append(&t);
    if !note.is_empty() {
        let n = label(note, &["caption", "dim-label"]);
        n.set_valign(gtk4::Align::End);
        h.append(&n);
    }
    body.append(&h);
}

fn grid(cols: u32) -> gtk4::FlowBox {
    let g = gtk4::FlowBox::new();
    g.set_homogeneous(true);
    g.set_max_children_per_line(cols);
    g.set_min_children_per_line(1);
    g.set_selection_mode(gtk4::SelectionMode::None);
    g.set_column_spacing(12);
    g.set_row_spacing(12);
    g
}

fn app_icon(a: Option<&App>, size: i32) -> gtk4::Image {
    let i = match a.and_then(|a| a.icon.as_ref()) {
        Some(p) => gtk4::Image::from_file(p),
        None => gtk4::Image::from_icon_name("applib-package-symbolic"),
    };
    i.set_pixel_size(size);
    i.set_valign(gtk4::Align::Start);
    i
}

fn verified_mark() -> gtk4::Image {
    let v = gtk4::Image::from_icon_name("applib-seal-check-symbolic");
    v.set_pixel_size(14);
    v.add_css_class("al-verified");
    v.set_tooltip_text(Some("Flathub checked that this publisher is who it says it is."));
    v
}

fn publisher(a: &App) -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let name = if a.developer.is_empty() { "Unknown publisher" } else { a.developer.as_str() };
    let l = label(name, &["al-dev", "dim-label"]);
    l.set_max_width_chars(20);
    l.set_wrap(false);
    l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    b.append(&l);
    if a.verified {
        b.append(&verified_mark());
    }
    b
}

/// One app card. `about` replaces Flathub's summary (ArkaOS's own line for its picks).
fn card(st: &S, a: Option<&App>, name: &str, about: &str, id: &str) -> gtk4::Button {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
    row.append(&app_icon(a, 52));
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    col.set_hexpand(true);
    let n = label(name, &["al-name"]);
    n.set_max_width_chars(18);
    n.set_wrap(false);
    n.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    col.append(&n);
    if let Some(a) = a {
        col.append(&publisher(a));
    }
    let s = label(about, &["al-sum"]);
    // FlowBox fits cards by natural width; keep it small so three fit on a laptop.
    s.set_max_width_chars(22);
    s.set_width_chars(16);
    s.set_margin_top(5);
    s.set_lines(2);
    s.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    col.append(&s);
    row.append(&col);
    let b = gtk4::Button::new();
    b.add_css_class("card");
    b.add_css_class("al-card");
    b.set_child(Some(&row));
    let st = st.clone();
    let id = id.to_string();
    b.connect_clicked(move |_| {
        let found = st.apps.borrow().iter().find(|a| a.id.eq_ignore_ascii_case(&id)).cloned();
        if let Some(a) = found {
            app_page(&st, &a);
        }
    });
    b
}

fn find<'a>(apps: &'a [App], id: &str) -> Option<&'a App> {
    apps.iter().find(|a| a.id.eq_ignore_ascii_case(id))
}

fn show(st: &S, idx: usize) {
    st.current.set(idx);
    st.nav.replace(&[]);
    show_section_page(st, idx);
    st.split.set_show_content(true);
}

fn show_section_page(st: &S, idx: usize) {
    let page = if idx == INSTALLED {
        installed_page(st)
    } else if idx == 0 || idx >= SECTIONS.len() {
        explore_page(st)
    } else {
        category_page(st, idx)
    };
    st.nav.push(&page);
}

fn status_block(st: &S, body: &gtk4::Box) -> bool {
    match &*st.load.borrow() {
        Load::Ready => true,
        Load::Loading => {
            let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
            b.set_margin_top(24);
            let sp = gtk4::Spinner::new();
            sp.start();
            b.append(&sp);
            b.append(&label("Getting the list of apps from Flathub…", &["dim-label"]));
            body.append(&b);
            false
        }
        Load::Failed(why) => {
            let b = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
            b.set_margin_top(24);
            b.append(&label("Couldn't get the list of apps from Flathub. Check the internet connection and try again.", &[]));
            b.append(&label(why, &["caption", "dim-label"]));
            let retry = gtk4::Button::with_label("Try again");
            retry.set_halign(gtk4::Align::Start);
            let s = st.clone();
            retry.connect_clicked(move |_| {
                *s.load.borrow_mut() = Load::Loading;
                refresh_page(&s);
                let s2 = s.clone();
                glib::spawn_future_local(async move { start(&s2).await });
            });
            b.append(&retry);
            body.append(&b);
            false
        }
    }
}

fn explore_page(st: &S) -> adw::NavigationPage {
    let (sc, body) = scaffold("Explore", "Apps from Flathub. Before you install one, its page shows what it can reach.");
    let ready = status_block(st, &body);
    let apps = st.apps.borrow().clone();
    if !st.picks.is_empty() {
        heading(&body, "Picked by ArkaOS", "A selection, not an audit");
        let g = grid(3);
        for p in &st.picks {
            g.insert(&card(st, find(&apps, &p.id), &p.name, &p.about, &p.id), -1);
        }
        body.append(&g);
    }
    if ready {
        let picked: Vec<String> = st.picks.iter().map(|p| p.id.to_lowercase()).collect();
        let mut recent: Vec<&App> = apps
            .iter()
            .filter(|a| a.verified && a.icon.is_some() && a.released > 0 && !picked.contains(&a.id.to_lowercase()))
            .collect();
        recent.sort_by_key(|a| -a.released);
        heading(&body, "New releases on Flathub", "From verified publishers, newest first");
        let g = grid(3);
        for a in recent.into_iter().take(9) {
            g.insert(&card(st, Some(a), &a.name, &a.summary, &a.id), -1);
        }
        body.append(&g);
    }
    adw::NavigationPage::new(&sc, "Explore")
}

fn in_section(a: &App, idx: usize) -> bool {
    let cats = SECTIONS[idx].2;
    let chat = a.categories.iter().any(|c| c == "InstantMessaging" || c == "Chat");
    // Messaging apps are Network apps too; keep them out of Internet.
    if idx == 2 && chat {
        return false;
    }
    a.categories.iter().any(|c| cats.contains(&c.as_str()))
}

fn paged_grid(st: &S, body: &gtk4::Box, list: Rc<Vec<usize>>) {
    let g = grid(3);
    body.append(&g);
    let more = gtk4::Button::with_label("Show more");
    more.set_halign(gtk4::Align::Center);
    more.set_margin_top(20);
    more.add_css_class("pill");
    body.append(&more);
    let shown = Rc::new(Cell::new(0usize));
    let fill = {
        let st = st.clone();
        let g = g.clone();
        let more = more.clone();
        let shown = shown.clone();
        move || {
            let apps = st.apps.borrow().clone();
            let from = shown.get();
            for &i in list.iter().skip(from).take(PAGE) {
                let a = &apps[i];
                g.insert(&card(&st, Some(a), &a.name, &a.summary, &a.id), -1);
            }
            shown.set((from + PAGE).min(list.len()));
            more.set_visible(shown.get() < list.len());
        }
    };
    fill();
    more.connect_clicked(move |_| fill());
}

fn category_page(st: &S, idx: usize) -> adw::NavigationPage {
    let title = SECTIONS[idx].0;
    let apps = st.apps.borrow().clone();
    let list: Vec<usize> = (0..apps.len()).filter(|&i| in_section(&apps[i], idx)).collect();
    let sub = if matches!(*st.load.borrow(), Load::Ready) {
        format!("{} apps on Flathub, A to Z.", list.len())
    } else {
        String::new()
    };
    let (sc, body) = scaffold(title, &sub);
    if status_block(st, &body) {
        let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        spacer.set_margin_top(22);
        body.append(&spacer);
        paged_grid(st, &body, Rc::new(list));
    }
    adw::NavigationPage::new(&sc, title)
}

fn results(st: &S, q: &str) {
    let apps = st.apps.borrow().clone();
    let words: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();
    let mut hits: Vec<(u8, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let name = a.name.to_lowercase();
            let hay = format!("{name} {} {} {}", a.summary.to_lowercase(), a.keywords.join(" "), a.id.to_lowercase());
            if !words.iter().all(|w| hay.contains(w.as_str())) {
                return None;
            }
            let q = words.join(" ");
            let rank = if name == q { 0 } else if name.starts_with(&q) { 1 } else if name.contains(&q) { 2 } else { 3 };
            Some((rank, i))
        })
        .collect();
    hits.sort();
    let title = format!("Results for “{q}”");
    let sub = match hits.len() {
        0 => String::new(),
        1 => "1 app".into(),
        n => format!("{n} apps"),
    };
    let (sc, body) = scaffold(&title, &sub);
    if status_block(st, &body) {
        if hits.is_empty() {
            let l = label(&format!("No app on Flathub matches “{q}”."), &["dim-label"]);
            l.set_margin_top(24);
            body.append(&l);
        } else {
            let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            spacer.set_margin_top(22);
            body.append(&spacer);
            paged_grid(st, &body, Rc::new(hits.into_iter().map(|(_, i)| i).collect()));
        }
    }
    st.nav.replace(&[adw::NavigationPage::new(&sc, &title)]);
    st.split.set_show_content(true);
}

// ── App page ─────────────────────────────────────────────────────────────────

fn fact(k: &str, v: &str) -> (gtk4::Box, gtk4::Label) {
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    b.append(&label(k, &["al-fact-k", "dim-label"]));
    let val = label(v, &["al-fact-v"]);
    b.append(&val);
    (b, val)
}

fn licence_text(spdx: &str) -> String {
    if spdx.is_empty() {
        "Not stated".into()
    } else if spdx.contains("LicenseRef-proprietary") {
        "Proprietary".into()
    } else {
        spdx.to_string()
    }
}

fn app_page(st: &S, a: &App) {
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.set_margin_top(28);
    body.set_margin_bottom(40);
    body.set_margin_start(36);
    body.set_margin_end(36);

    let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 22);
    top.append(&app_icon(Some(a), 96));
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    col.set_hexpand(true);
    col.set_valign(gtk4::Align::Center);
    col.append(&label(&a.name, &["al-h1"]));
    col.append(&publisher(a));
    col.append(&label(&a.summary, &["al-sub"]));
    top.append(&col);
    let actions = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    actions.set_valign(gtk4::Align::Center);
    top.append(&actions);
    body.append(&top);
    let note = label("", &["caption"]);
    note.set_xalign(1.0);
    note.set_visible(false);
    note.set_margin_top(8);
    body.append(&note);
    fill_actions(st, a, &actions, &note);

    let facts = gtk4::Box::new(gtk4::Orientation::Horizontal, 40);
    facts.set_margin_top(26);
    let (f1, version) = fact("Version", if a.version.is_empty() { "Not stated" } else { &a.version });
    let (f2, dl) = fact("Download", "…");
    let (f3, disk) = fact("On disk", "…");
    let (f4, _) = fact("Licence", &licence_text(&a.license));
    for f in [&f1, &f2, &f3, &f4] {
        facts.append(f);
    }
    body.append(&facts);

    let cols = grid(2);
    cols.set_homogeneous(false);
    cols.set_column_spacing(40);
    cols.set_margin_top(18);
    let reach = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    reach.set_hexpand(true);
    heading(&reach, "What it can reach", "From the app's Flatpak permissions");
    let reach_list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let wait = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let sp = gtk4::Spinner::new();
    sp.start();
    wait.append(&sp);
    wait.append(&label("Asking Flathub…", &["dim-label"]));
    reach_list.append(&wait);
    reach.append(&reach_list);
    cols.insert(&reach, -1);

    let about = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    heading(&about, "About", "");
    for p in a.description.iter().take(12) {
        match p {
            Para::P(t) => about.append(&label(t, &[])),
            Para::Li(t) => about.append(&label(&format!("•  {t}"), &[])),
        }
    }
    if a.description.is_empty() {
        about.append(&label(&a.summary, &[]));
    }
    let src = label(&format!("Source: Flathub, {}. Installs for your account only.", a.id), &["caption", "dim-label"]);
    src.set_margin_top(8);
    about.append(&src);
    if !a.homepage.is_empty() {
        let host = a.homepage.split("//").nth(1).unwrap_or(&a.homepage).trim_end_matches('/').to_string();
        let link = gtk4::LinkButton::with_label(&a.homepage, &host);
        link.set_halign(gtk4::Align::Start);
        about.append(&link);
    }
    cols.insert(&about, -1);
    body.append(&cols);

    let clamp = adw::Clamp::builder().maximum_size(1120).tightening_threshold(900).child(&body).build();
    let sc = gtk4::ScrolledWindow::new();
    sc.set_hscrollbar_policy(gtk4::PolicyType::Never);
    sc.set_child(Some(&clamp));
    st.nav.push(&adw::NavigationPage::new(&sc, &a.name));
    st.split.set_show_content(true);

    let id = a.id.clone();
    let scope = st.scope.get();
    glib::spawn_future_local(async move {
        let (ok, out, err) = run(&["flatpak", "remote-info", scope, "flathub", &id]).await;
        let pick = |key: &str| {
            out.lines().find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
        };
        if ok {
            dl.set_label(&pick("Download Size:").unwrap_or_else(|| "Not stated".into()));
            disk.set_label(&pick("Installed Size:").unwrap_or_else(|| "Not stated".into()));
            if let Some(v) = pick("Version:") {
                version.set_label(&v);
            }
        } else {
            dl.set_label("Unavailable");
            disk.set_label("Unavailable");
        }
        let (ok, meta, err2) = run(&["flatpak", "remote-info", "--show-metadata", scope, "flathub", &id]).await;
        while let Some(c) = reach_list.first_child() {
            reach_list.remove(&c);
        }
        if !ok {
            let why = if err2.is_empty() { err } else { err2 };
            reach_list.append(&label(&format!("Couldn't ask Flathub: {}", last_line(&why)), &["al-err"]));
            reach_list.append(&label("Nothing is installed without this list. Check the internet connection and open the app again.", &["caption", "dim-label"]));
            return;
        }
        for r in perms::parse(&meta) {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
            row.add_css_class("al-reach-row");
            let i = ic(r.icon);
            i.set_pixel_size(18);
            i.set_valign(gtk4::Align::Start);
            i.add_css_class("dim-label");
            row.append(&i);
            let t = label(&r.text, &[]);
            t.set_hexpand(true);
            row.append(&t);
            if r.wide {
                let w = label("Broad access", &["al-wide"]);
                w.set_valign(gtk4::Align::Start);
                row.append(&w);
            }
            reach_list.append(&row);
        }
        let c = label(perms::CAVEAT, &["caption", "dim-label"]);
        c.set_margin_top(12);
        reach_list.append(&c);
    });
}

fn clear(b: &gtk4::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn fill_actions(st: &S, a: &App, actions: &gtk4::Box, note: &gtk4::Label) {
    clear(actions);
    let inst = st.installed.borrow().get(&a.id).cloned();
    match inst {
        None => {
            let b = gtk4::Button::with_label("Install");
            b.add_css_class("suggested-action");
            b.add_css_class("pill");
            actions.append(&b);
            let (st, a, actions, note) = (st.clone(), a.clone(), actions.clone(), note.clone());
            b.connect_clicked(move |_| {
                busy(&actions, "Installing…");
                note.set_visible(true);
                note.remove_css_class("al-err");
                note.set_label("Large apps can take several minutes.");
                let (st, a, actions, note) = (st.clone(), a.clone(), actions.clone(), note.clone());
                glib::spawn_future_local(async move {
                    let (ok, out, err) = run(&["arka", "install", &a.id, "--yes", "--any"]).await;
                    refresh_installed(&st).await;
                    if ok {
                        note.set_visible(false);
                    } else {
                        note.add_css_class("al-err");
                        note.set_label(&format!("Not installed: {}", last_line(if err.is_empty() { &out } else { &err })));
                    }
                    fill_actions(&st, &a, &actions, &note);
                });
            });
        }
        Some(i) => {
            let open = gtk4::Button::with_label("Open");
            open.add_css_class("suggested-action");
            open.add_css_class("pill");
            let id = a.id.clone();
            open.connect_clicked(move |_| launch(&id));
            actions.append(&open);
            if i.user {
                let rm = gtk4::Button::with_label("Remove");
                rm.add_css_class("flat");
                let (st2, a2, actions2, note2) = (st.clone(), a.clone(), actions.clone(), note.clone());
                rm.connect_clicked(move |_| confirm_remove(&st2, &a2, &actions2, &note2));
                actions.append(&rm);
            } else {
                actions.append(&label("Installed for every account", &["caption", "dim-label"]));
            }
        }
    }
    test_step(st, actions);
}

// ── Test hook ────────────────────────────────────────────────────────────────
// ARKA_APPLIB_ACT=install,remove presses those buttons on the open app page, one per state,
// with the same click handlers a mouse uses; ARKA_APPLIB_SHOTS=<dir> saves a PNG of every
// state, and the app quits after the last one. Does nothing when ARKA_APPLIB_ACT is unset.
thread_local! {
    static STEP: Cell<usize> = const { Cell::new(0) };
}

fn shot(window: &adw::ApplicationWindow, name: &str) {
    let Some(dir) = std::env::var_os("ARKA_APPLIB_SHOTS") else { return };
    let paintable = gtk4::WidgetPaintable::new(Some(window));
    let snap = gtk4::Snapshot::new();
    paintable.snapshot(&snap, window.width() as f64, window.height() as f64);
    if let (Some(node), Some(r)) = (snap.to_node(), window.native().and_then(|n| n.renderer())) {
        let _ = r.render_texture(&node, None).save_to_png(PathBuf::from(dir).join(format!("{name}.png")));
    }
}

fn test_step(st: &S, actions: &gtk4::Box) {
    let Ok(plan) = std::env::var("ARKA_APPLIB_ACT") else { return };
    let steps: Vec<String> = plan.split(',').map(String::from).collect();
    let n = STEP.with(|c| c.get());
    let (st, actions) = (st.clone(), actions.clone());
    glib::timeout_add_local_once(std::time::Duration::from_millis(3000), move || {
        shot(&st.window, &format!("step{n}"));
        let Some(want) = steps.get(n) else {
            st.window.close();
            return;
        };
        STEP.with(|c| c.set(n + 1));
        let mut w = actions.first_child();
        while let Some(x) = w {
            if let Ok(b) = x.clone().downcast::<gtk4::Button>() {
                if b.label().is_some_and(|l| l.eq_ignore_ascii_case(want)) {
                    b.emit_clicked();
                    return;
                }
            }
            w = x.next_sibling();
        }
        eprintln!("test: no {want} button");
        st.window.close();
    });
}

fn busy(actions: &gtk4::Box, what: &str) {
    clear(actions);
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let sp = gtk4::Spinner::new();
    sp.start();
    b.append(&sp);
    b.append(&gtk4::Label::new(Some(what)));
    actions.append(&b);
}

fn confirm_remove(st: &S, a: &App, actions: &gtk4::Box, note: &gtk4::Label) {
    let d = adw::AlertDialog::new(
        Some(&format!("Remove {}?", a.name)),
        Some("Its saved data and settings stay in your home folder, so installing it again picks up where you left off."),
    );
    d.add_response("cancel", "Cancel");
    d.add_response("remove", "Remove");
    d.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    d.set_close_response("cancel");
    let (st2, a, actions, note) = (st.clone(), a.clone(), actions.clone(), note.clone());
    d.connect_response(None, move |_, r| {
        if r != "remove" {
            return;
        }
        busy(&actions, "Removing…");
        let (st, a, actions, note) = (st2.clone(), a.clone(), actions.clone(), note.clone());
        glib::spawn_future_local(async move {
            let (ok, out, err) = run(&["arka", "remove", &a.id]).await;
            refresh_installed(&st).await;
            if !ok {
                note.set_visible(true);
                note.add_css_class("al-err");
                note.set_label(&format!("Not removed: {}", last_line(if err.is_empty() { &out } else { &err })));
            }
            fill_actions(&st, &a, &actions, &note);
        });
    });
    d.present(Some(&st.window));
    if std::env::var_os("ARKA_APPLIB_ACT").is_some() {
        let (d, w) = (d.clone(), st.window.clone());
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            shot(&w, "confirm-remove");
            d.emit_by_name::<()>("response", &[&"remove"]);
            d.force_close();
        });
    }
}

fn launch(id: &str) {
    let _ = gio::Subprocess::newv(
        &[std::ffi::OsStr::new("flatpak"), std::ffi::OsStr::new("run"), std::ffi::OsStr::new(id)],
        gio::SubprocessFlags::NONE,
    );
}

// ── Installed & updates ──────────────────────────────────────────────────────

fn installed_icon(st: &S, id: &str) -> gtk4::Image {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    for base in [home.join(".local/share/flatpak/exports/share/icons/hicolor"), PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor")] {
        for (dir, ext) in [("128x128", "png"), ("scalable", "svg"), ("256x256", "png"), ("64x64", "png")] {
            let p = base.join(dir).join("apps").join(format!("{id}.{ext}"));
            if p.is_file() {
                let i = gtk4::Image::from_file(p);
                i.set_pixel_size(40);
                return i;
            }
        }
    }
    let apps = st.apps.borrow().clone();
    app_icon(find(&apps, id), 40)
}

/// Update one app, or all of them, then show the Installed page again. What was updated leaves
/// the list at once, so it isn't offered again while Flathub is asked for what's left.
async fn update(st: &S, id: Option<&str>) {
    let mut cmd = vec!["flatpak", "update", "--user", "-y", "--noninteractive"];
    cmd.extend(id);
    let (ok, out, err) = run(&cmd).await;
    *st.update_error.borrow_mut() = (!ok).then(|| last_line(if err.is_empty() { &out } else { &err }));
    if ok {
        st.updates.borrow_mut().retain(|u| id.is_some_and(|id| u.id != id));
    }
    refresh_installed(st).await;
    check_updates(st).await;
    if std::env::var_os("ARKA_APPLIB_ACT").is_some() {
        STEP.with(|c| c.set(2));
    }
    show(st, INSTALLED);
}

fn installed_page(st: &S) -> adw::NavigationPage {
    let (sc, body) = scaffold("Installed & updates", "");
    let ups = st.updates.borrow().clone();
    let inst = st.installed.borrow().clone();

    let h = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    h.set_margin_top(30);
    h.set_margin_bottom(14);
    let t = label("Updates", &["al-h2"]);
    t.set_hexpand(true);
    h.append(&t);
    let again = gtk4::Button::with_label("Check again");
    h.append(&again);
    let all = gtk4::Button::with_label("Update all");
    all.add_css_class("suggested-action");
    all.set_sensitive(!ups.is_empty());
    h.append(&all);
    body.append(&h);
    let failed = label("", &["caption", "al-err"]);
    failed.set_visible(false);
    failed.set_margin_bottom(10);
    body.append(&failed);
    if let Some(why) = st.update_error.borrow().as_ref() {
        failed.set_label(&format!("Not updated: {why}"));
        failed.set_visible(true);
    }
    {
        let st = st.clone();
        again.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label("Checking…");
            let st = st.clone();
            glib::spawn_future_local(async move {
                check_updates(&st).await;
                show(&st, INSTALLED);
            });
        });
    }
    {
        let st = st.clone();
        all.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label("Updating…");
            let st = st.clone();
            glib::spawn_future_local(async move { update(&st, None).await });
        });
    }

    // Test hook: press Update all once updates are listed (step 0 -> 1); update() sets step 2
    // when flatpak has returned, and the page it draws then is the "after" picture.
    if std::env::var("ARKA_APPLIB_ACT").as_deref() == Ok("update-all") {
        let n = STEP.with(|c| c.get());
        let (w, all) = (st.window.clone(), all.clone());
        if n == 0 && !ups.is_empty() {
            STEP.with(|c| c.set(1));
            glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                shot(&w, "updates-before");
                all.emit_clicked();
            });
        } else if n == 2 {
            glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                shot(&w, "updates-after");
                w.close();
            });
        }
    }

    if ups.is_empty() {
        body.append(&label("No updates for the apps you installed.", &["dim-label"]));
    } else {
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::None);
        for u in &ups {
            let i = inst.get(&u.id);
            let name = i.map(|i| i.name.clone()).unwrap_or_else(|| u.id.clone());
            let from = i.map(|i| i.version.clone()).unwrap_or_default();
            let sub = match (from.is_empty(), u.version.is_empty()) {
                (false, false) if from == u.version => format!("A newer build of {from}"),
                (false, false) => format!("{from} → {}", u.version),
                (true, false) => format!("New version {}", u.version),
                _ => "A newer build is available".into(),
            };
            let row = adw::ActionRow::builder().title(glib::markup_escape_text(&name).as_str()).subtitle(sub).build();
            row.add_prefix(&installed_icon(st, &u.id));
            if !u.size.is_empty() {
                let s = label(&u.size, &["dim-label", "caption"]);
                s.set_wrap(false);
                s.set_valign(gtk4::Align::Center);
                row.add_suffix(&s);
            }
            let b = gtk4::Button::with_label("Update");
            b.set_valign(gtk4::Align::Center);
            let (st2, id) = (st.clone(), u.id.clone());
            b.connect_clicked(move |b| {
                b.set_sensitive(false);
                b.set_label("Updating…");
                let (st, id) = (st2.clone(), id.clone());
                glib::spawn_future_local(async move { update(&st, Some(&id)).await });
            });
            row.add_suffix(&b);
            list.append(&row);
        }
        body.append(&list);
    }
    let checked = st.checked.borrow().clone();
    let foot = label(format!("{checked} ArkaOS itself updates separately, in System Updates.").trim(), &["caption", "dim-label"]);
    foot.set_margin_top(10);
    body.append(&foot);

    heading(&body, "Installed apps", "A to Z");
    let mut ids: Vec<(String, Inst)> = inst.into_iter().collect();
    ids.sort_by_key(|(_, i)| i.name.to_lowercase());
    if ids.is_empty() {
        body.append(&label("Apps you install from App Library appear here.", &["dim-label"]));
    } else {
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::None);
        let apps = st.apps.borrow().clone();
        for (id, i) in ids {
            let sub = if i.user { i.version.clone() } else { format!("{}  ·  installed for every account", i.version) };
            let row = adw::ActionRow::builder().title(glib::markup_escape_text(&i.name).as_str()).subtitle(sub.trim_start_matches("  ·  ")).build();
            row.add_prefix(&installed_icon(st, &id));
            let open = gtk4::Button::with_label("Open");
            open.set_valign(gtk4::Align::Center);
            let id2 = id.clone();
            open.connect_clicked(move |_| launch(&id2));
            row.add_suffix(&open);
            if let Some(a) = find(&apps, &id).cloned() {
                row.set_activatable(true);
                let st = st.clone();
                row.connect_activated(move |_| app_page(&st, &a));
            }
            list.append(&row);
        }
        body.append(&list);
    }
    adw::NavigationPage::new(&sc, "Installed & updates")
}

fn about(st: &S) {
    let d = adw::AboutDialog::builder()
        .application_name("App Library")
        .application_icon("arka-capsule")
        .developer_name("ArkaOS contributors")
        .developers(vec!["Thulasi Ram K", "ArkaOS contributors"])
        .license_type(gtk4::License::Gpl30)
        .website("https://github.com/thulasiramk-2310/Arka")
        .issue_url("https://github.com/thulasiramk-2310/Arka/issues")
        .comments("Finds apps on Flathub and installs them for your account only, after showing what each one can reach. The apps ArkaOS picks are a selection, not an audit: nobody at ArkaOS has reviewed their code.")
        .build();
    d.add_credit_section(Some("Icons"), &["Phosphor Icons (MIT) https://phosphoricons.com"]);
    d.present(Some(&st.window));
}
