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

/// Palette + base widget styling shared by every shell surface.
/// Prepend to a component's local CSS: `format!("{}{}", theme::TOKENS, LOCAL)`.
pub const TOKENS: &str = "
/* ── palette ─────────────────────────────────────────── */
@define-color bg_base    #0c0c0e;
@define-color bg_raised  #111113;
@define-color bg_overlay #17171a;
@define-color bg_sunken  #0a0a0c;
@define-color border_sub  #1f1f23;
@define-color border_ui   #27272c;
@define-color border_emph #3a3a42;
@define-color text_hi    #ededf0;
@define-color text_lo    #7d7d8a;
@define-color text_muted #3d3d48;
@define-color accent     #22c55e;
@define-color accent_dim #15803d;
@define-color danger     #ef4444;
@define-color warn       #f59e0b;
@define-color info       #3b82f6;

/* ── base surfaces ───────────────────────────────────── */
window { background-color: @bg_base; color: @text_hi; }
.surface-transparent { background: transparent; }

/* scrollbars — 4px, sunken track, emph thumb */
scrollbar { background: transparent; }
scrollbar trough { background-color: @bg_sunken; border-radius: 4px; min-width: 4px; min-height: 4px; }
scrollbar slider { background-color: @border_emph; border-radius: 4px; min-width: 4px; min-height: 4px; }
scrollbar slider:hover { background-color: #4a4a54; }

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
";

/// libadwaita named-color overrides — repaints Adwaita apps (Settings,
/// Dashboard) in the ArkaOS palette. Adwaita resolves these names internally,
/// so overriding them is the supported way to re-skin without fighting the
/// stylesheet. Accent is green (privacy = safe); everything else stays neutral.
pub const ADW_OVERRIDES: &str = "
@define-color window_bg_color    #0c0c0e;
@define-color window_fg_color    #ededf0;
@define-color view_bg_color      #0c0c0e;
@define-color view_fg_color      #ededf0;
@define-color headerbar_bg_color #111113;
@define-color headerbar_fg_color #ededf0;
@define-color sidebar_bg_color   #0a0a0c;
@define-color sidebar_fg_color   #ededf0;
@define-color card_bg_color      #17171a;
@define-color card_fg_color      #ededf0;
@define-color dialog_bg_color    #111113;
@define-color popover_bg_color   #17171a;
@define-color popover_fg_color   #ededf0;
@define-color accent_color       #2ec36a;
@define-color accent_bg_color    #22c55e;
@define-color accent_fg_color    #0a0a0c;
@define-color destructive_color  #ef4444;
@define-color destructive_bg_color #ef4444;
@define-color destructive_fg_color #0a0a0c;
@define-color borders            #27272c;
";

/// Install the shared tokens as a low-priority base provider on the default
/// display. Components still add their own provider on top for local styling.
pub fn install_base() {
    install(TOKENS, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION - 1);
}

/// Install the libadwaita palette overrides plus the shared tokens. Call this
/// (instead of [`install_base`]) from adwaita apps after setting ForceDark.
pub fn install_adw() {
    install(ADW_OVERRIDES, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    install_base();
}

fn install(css: &str, priority: u32) {
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(css);
        gtk4::style_context_add_provider_for_display(&display, &provider, priority);
    }
}
