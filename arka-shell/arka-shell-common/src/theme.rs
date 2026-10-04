//! Shared ArkaOS design system — single source of truth for the shell's look.
//!
//! GTK4 CSS quirks this works around:
//!   - no `var(--x)`        → palette via `@define-color`
//!   - no `backdrop-filter` → near-opaque solid panels instead of blur
//!   - no `text-transform`  → uppercase labels in Rust before display
//!   - no per-glyph emoji   → symbolic icon-theme names, recolored via `color:`
//!
//! Every component prepends [`TOKENS`] to its own CSS so colors stay consistent.
//! Spacing follows a strict 4px grid; radii: 4 / 6 / 10 / 14; one shadow direction.

/// Palette for the Dark look: neutral graphite, matching the ArkaOS Dark
/// Plasma colour scheme (ArkaOS.colors).
pub const PALETTE_DARK: &str = "
@define-color bg_base    #17191d;
@define-color bg_raised  #1b1d22;
@define-color bg_overlay #202329;
@define-color bg_sunken  #121418;
@define-color border_sub  #22252b;
@define-color border_ui   #2a2d34;
@define-color border_emph #3b3e45;
@define-color text_hi    #f1f1f1;
@define-color text_lo    #a6a6a6;
@define-color text_muted #8c8c8c;
@define-color accent     #16c784;
@define-color accent_dim #0e8f5d;
@define-color secondary  #60a5fa;
@define-color danger     #ff8074;
@define-color warn       #fbbf5c;
@define-color info       #60a5fa;
";

/// Palette for the Light look, matching ArkaOSLight.colors. Text-coloured
/// greens are darker than the brand green so they stay readable on white.
pub const PALETTE_LIGHT: &str = "
@define-color bg_base    #ffffff;
@define-color bg_raised  #f6f7f9;
@define-color bg_overlay #f2f4f7;
@define-color bg_sunken  #eceff3;
@define-color border_sub  #eceff3;
@define-color border_ui   #e3e6ea;
@define-color border_emph #c9d0d9;
@define-color text_hi    #111111;
@define-color text_lo    #5c5c5c;
@define-color text_muted #737373;
@define-color accent     #0e8a5f;
@define-color accent_dim #0b6e4b;
@define-color secondary  #0f6cbd;
@define-color danger     #b42318;
@define-color warn       #b45309;
@define-color info       #0f6cbd;
";

/// Base widget styling shared by every shell surface (uses a PALETTE_*).
/// Prepend to a component's local CSS: `format!("{}{}", theme::TOKENS, LOCAL)`.
pub const TOKENS: &str = "
/* ── base surfaces ───────────────────────────────────── */
window { background-color: @bg_base; color: @text_hi; }
.surface-transparent { background: transparent; }

/* scrollbars — 4px, sunken track, emph thumb */
scrollbar { background: transparent; }
scrollbar trough { background-color: @bg_sunken; border-radius: 4px; min-width: 4px; min-height: 4px; }
scrollbar slider { background-color: @border_emph; border-radius: 4px; min-width: 4px; min-height: 4px; }
scrollbar slider:hover { background-color: @text_muted; }

/* ── shared primitives ───────────────────────────────── */
.card {
    background-color: @bg_overlay;
    border: 1px solid @border_ui;
    border-radius: 10px;
}
.row-flat {
    background: transparent;
    border-bottom: 1px solid @border_sub;
}

/* status dots (privacy = safe = green) */
.dot { min-width: 6px; min-height: 6px; border-radius: 999px; }
.dot-ok   { background-color: @accent; }
.dot-warn { background-color: @warn; }
.dot-off  { background-color: @text_muted; }

/* badges — small, tinted, no border */
.badge {
    font-size: 10px; font-weight: 600;
    padding: 2px 7px; border-radius: 4px;
    background-color: alpha(@accent, 0.10); color: @accent;
}
.badge-foss { background-color: alpha(@info, 0.12); color: @info; }

/* mono helper for technical values / logs */
.mono { font-family: 'JetBrains Mono', monospace; }

/* label scale */
.label-xs    { font-size: 10px; font-weight: 600; color: @text_muted; }
.label-meta  { font-size: 12px; color: @text_lo; }
.label-body  { font-size: 13px; color: @text_hi; }
.label-row   { font-size: 15px; font-weight: 600; color: @text_hi; }
.label-head  { font-size: 20px; font-weight: 600; color: @text_hi; }

/* toggle — 32x18, smooth */
.arka-toggle { min-width: 32px; min-height: 18px; border-radius: 999px; background-color: @bg_sunken; border: 1px solid @border_ui; }
.arka-toggle:checked { background-color: @accent; border-color: @accent; }
.arka-toggle slider { min-width: 14px; min-height: 14px; border-radius: 999px; background: #ffffff; margin: 1px; }

/* ── ArkaOS page language (same as setup + Privacy dashboard) ──────────
   Serif page titles, flat rows split by hairlines instead of cards, quiet
   ink-coloured buttons instead of bright fills. */
.arka-title { font-family: 'Noto Serif', serif; font-size: 30px; font-weight: 500; color: @text_hi; }
.arka-sub   { font-size: 14px; color: @text_lo; }
list.boxed-list {
    background: transparent; box-shadow: none; border-radius: 0;
    border: none; border-top: 1px solid @border_ui; border-bottom: 1px solid @border_ui;
}
list.boxed-list > row { background: transparent; border-radius: 0; border-bottom: 1px solid @border_sub; }
list.boxed-list > row:last-child { border-bottom: none; }
list.boxed-list > row:hover { background-color: alpha(@text_hi, 0.04); }
preferencesgroup > box > box > box > label.heading,
preferencesgroup .heading { font-size: 13px; font-weight: 600; color: @text_lo; }
button.suggested-action, button.pill.suggested-action {
    background-color: @text_hi; color: @bg_base; box-shadow: none; border-radius: 999px;
}
button.suggested-action:hover { background-color: alpha(@text_hi, 0.85); }
button.destructive-action { background-color: transparent; color: @danger; box-shadow: inset 0 0 0 1px alpha(@danger, 0.5); }
";

/// libadwaita named-color overrides — repaints Adwaita apps (Settings,
/// Dashboard) in the ArkaOS palette. Adwaita resolves these names internally,
/// so overriding them is the supported way to re-skin without fighting the
/// stylesheet. Accent is green (privacy = safe); everything else stays neutral.
pub const ADW_OVERRIDES: &str = "
@define-color window_bg_color    #17191d;
@define-color window_fg_color    #f1f1f1;
@define-color view_bg_color      #1b1d22;
@define-color view_fg_color      #f1f1f1;
@define-color headerbar_bg_color #17191d;
@define-color headerbar_fg_color #f1f1f1;
@define-color sidebar_bg_color   #121418;
@define-color sidebar_fg_color   #f1f1f1;
@define-color card_bg_color      #202329;
@define-color card_fg_color      #f1f1f1;
@define-color dialog_bg_color    #1b1d22;
@define-color popover_bg_color   #202329;
@define-color popover_fg_color   #f1f1f1;
@define-color accent_color       #3ddc97;
@define-color accent_bg_color    #16c784;
@define-color accent_fg_color    #07080e;
@define-color destructive_color  #ff8074;
@define-color destructive_bg_color #e5484d;
@define-color destructive_fg_color #ffffff;
@define-color borders            #2a2d34;
";

/// libadwaita overrides for the Light look.
pub const ADW_OVERRIDES_LIGHT: &str = "
@define-color window_bg_color    #ffffff;
@define-color window_fg_color    #111111;
@define-color view_bg_color      #ffffff;
@define-color view_fg_color      #111111;
@define-color headerbar_bg_color #f6f7f9;
@define-color headerbar_fg_color #111111;
@define-color sidebar_bg_color   #f6f7f9;
@define-color sidebar_fg_color   #111111;
@define-color card_bg_color      #f6f7f9;
@define-color card_fg_color      #111111;
@define-color dialog_bg_color    #ffffff;
@define-color popover_bg_color   #ffffff;
@define-color popover_fg_color   #111111;
@define-color accent_color       #0e8a5f;
@define-color accent_bg_color    #16c784;
@define-color accent_fg_color    #07080e;
@define-color destructive_color  #b42318;
@define-color destructive_bg_color #c42b1c;
@define-color destructive_fg_color #ffffff;
@define-color borders            #e3e6ea;
";

/// Install the Dark palette and shared tokens as a low-priority base provider.
/// Components still add their own provider on top for local styling.
pub fn install_base() {
    install(&format!("{PALETTE_DARK}{TOKENS}"), gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION - 1);
}

/// Dark-only libadwaita palette + tokens (kept for callers that force dark).
pub fn install_adw() {
    install(ADW_OVERRIDES, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    install_base();
}

/// The full stylesheet for one look: libadwaita overrides, palette, tokens.
pub fn stylesheet(dark: bool) -> String {
    if dark {
        format!("{ADW_OVERRIDES}{PALETTE_DARK}{TOKENS}")
    } else {
        format!("{ADW_OVERRIDES_LIGHT}{PALETTE_LIGHT}{TOKENS}")
    }
}

fn install(css: &str, priority: u32) {
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(css);
        gtk4::style_context_add_provider_for_display(&display, &provider, priority);
    }
}

/// Follow the desktop's Light or Dark look (chosen in setup, changeable in
/// Settings) and repaint live when it changes. Call once from an adwaita app's
/// activate handler, instead of forcing dark. ARKA_LOOK=light|dark overrides
/// the system for previews and tests.
#[cfg(feature = "adw")]
pub fn follow_look() {
    use libadwaita as adw;
    let manager = adw::StyleManager::default();
    manager.set_color_scheme(match std::env::var("ARKA_LOOK").as_deref() {
        Ok("light") => adw::ColorScheme::ForceLight,
        Ok("dark") => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
    let Some(display) = gtk4::gdk::Display::default() else { return };
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(&stylesheet(manager.is_dark()));
    gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION - 1);
    manager.connect_dark_notify(move |m| provider.load_from_data(&stylesheet(m.is_dark())));
}

/// Preview hook for design review: with ARKA_SHOT=<file.png> set, render
/// `window` to that PNG after ARKA_SHOT_MS (default 2500) and quit the app.
/// Does nothing otherwise.
#[cfg(feature = "adw")]
pub fn preview_shot(window: &impl gtk4::prelude::IsA<gtk4::Window>) {
    use gtk4::prelude::*;
    let Ok(path) = std::env::var("ARKA_SHOT") else { return };
    let wait = std::env::var("ARKA_SHOT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(2500);
    let w: gtk4::Window = window.clone().upcast();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(wait), move || {
        let paintable = gtk4::WidgetPaintable::new(Some(&w));
        let snap = gtk4::Snapshot::new();
        paintable.snapshot(&snap, w.width() as f64, w.height() as f64);
        if let (Some(node), Some(r)) = (snap.to_node(), w.native().and_then(|n| n.renderer())) {
            let _ = r.render_texture(&node, None).save_to_png(&path);
        }
        if let Some(app) = w.application() { app.quit() } else { std::process::exit(0) }
    });
}

/// The shared page header: serif title + one-line explanation, left-aligned,
/// exactly as on the setup screens and the Privacy dashboard.
#[cfg(feature = "gtk")]
pub fn page_header(title: &str, sub: &str) -> gtk4::Box {
    use gtk4::prelude::*;
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    b.set_margin_bottom(8);
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("arka-title");
    t.set_xalign(0.0);
    let s = gtk4::Label::new(Some(sub));
    s.add_css_class("arka-sub");
    s.set_xalign(0.0);
    s.set_wrap(true);
    b.append(&t);
    b.append(&s);
    b
}
