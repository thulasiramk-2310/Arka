//! The installer's look: the same paper-white / graphite pages, serif titles and pill buttons
//! as ArkaOS Setup (arka-oobe), so installing and first start read as one product. The base
//! rules are copied from arka-oobe's css(); the `ai-` rules are the installer's own.

use std::cell::Cell;

thread_local! {
    /// How much bigger than the 1280x800 design every size is drawn on this screen.
    pub static SCALE: Cell<f64> = const { Cell::new(1.0) };
}

/// A design size in pixels, scaled to this screen.
pub fn sp(px: i32) -> i32 {
    SCALE.with(|s| (px as f64 * s.get()).round() as i32)
}

/// Same rule as the setup: grow with whichever side has less room, up to 1.5x.
pub fn scale_for(w: i32, h: i32) -> f64 {
    let s = (w as f64 / 1280.0).min(h as f64 / 820.0).clamp(1.0, 1.5);
    (s * 20.0).round() / 20.0
}

/// Every `NNpx` size of 3px or more in a style sheet, multiplied by `s`. Hairlines stay.
pub fn scale_css(css: &str, s: f64) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("px") {
        let head = &rest[..i];
        let start = head.len() - head.bytes().rev().take_while(|b| b.is_ascii_digit() || *b == b'.').count();
        let num = &head[start..];
        out.push_str(&head[..start]);
        match num.parse::<f64>() {
            Ok(v) if v >= 3.0 => out.push_str(&format!("{}", (v * s).round())),
            _ => out.push_str(num),
        }
        out.push_str("px");
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

pub fn css(dark: bool) -> String {
    let (bg, fg, muted, faint, line, hover, pfg, phover, off) = if dark {
        ("#101114", "#f1f1f1", "#a6a6a6", "#8c8c8c", "#3b3e45", "#1c1e22", "#101114", "#d6d6d6", "#3b3e45")
    } else {
        ("#f3f2ee", "#111111", "#5c5c5c", "#6b6b6b", "#d0cfca", "#e8e7e3", "#f3f2ee", "#2b2b2b", "#d6d5d0")
    };
    let fieldline = if dark { "#6b6f78" } else { "#7a7a74" };
    let err = if dark { "#ff8a80" } else { "#b42318" };
    let errbg = if dark { "#e5484d" } else { "#b42318" };
    let errhover = if dark { "#f06a6e" } else { "#8f1c13" };
    let ok = if dark { "#3ddc97" } else { "#007c3d" };
    let warn = if dark { "#fbbf5c" } else { "#b45309" };
    let warnbg = if dark { "#24201a" } else { "#f4ead9" };
    format!(
        "
window, stack, .oobe-root, .oobe-page {{ background-color: {bg}; color: {fg}; }}
* {{ font-family: 'Inter', 'Noto Sans', sans-serif; }}

.oobe-brand {{ font-size: 17px; font-weight: 600; color: {fg}; }}
.oobe-step {{ font-size: 15px; color: {muted}; }}
.oobe-title {{ font-family: 'Noto Serif', serif; font-size: 44px; font-weight: 500; color: {fg}; }}
.oobe-sub {{ font-size: 17px; color: {muted}; }}
.oobe-note {{ font-size: 14px; color: {faint}; }}
.oobe-err {{ font-size: 14px; color: {err}; }}
.oobe-tstep {{ font-size: 17px; color: {faint}; }}
.oobe-tstep.now {{ color: {fg}; font-weight: 600; }}
.oobe-tstep.done {{ color: {muted}; }}

entry.oobe-field, passwordentry.oobe-field {{
  background: none; background-color: transparent; color: {fg};
  border: none; border-bottom: 1.5px solid {fieldline}; border-radius: 0;
  box-shadow: none; outline: none; font-size: 22px; min-height: 52px; padding: 0 2px;
}}
entry.oobe-field:focus-within, passwordentry.oobe-field:focus-within {{ border-bottom: 1.5px solid {fg}; box-shadow: none; outline: none; }}
entry.oobe-field text, passwordentry.oobe-field text {{ background: none; color: {fg}; caret-color: {fg}; }}
entry.oobe-field text placeholder, passwordentry.oobe-field text placeholder {{ color: {faint}; opacity: 1; }}
passwordentry.oobe-field image {{ color: {faint}; -gtk-icon-size: 18px; }}
dropdown.oobe-field > button {{
  background: none; background-color: transparent; color: {fg};
  border: none; border-bottom: 1.5px solid {fieldline}; border-radius: 0;
  box-shadow: none; outline: none; min-height: 52px; padding: 0 2px;
}}
dropdown.oobe-field > button:hover, dropdown.oobe-field > button:focus-visible, dropdown.oobe-field > button:checked {{
  background: none; background-color: transparent; border-bottom: 1.5px solid {fg}; box-shadow: none; outline: none;
}}
dropdown.oobe-field > button label {{ font-size: 22px; font-weight: 400; color: {fg}; }}
dropdown.oobe-field > button image {{ color: {faint}; }}

button.oobe-primary {{
  background: none; background-color: {fg}; color: {pfg};
  border: none; border-radius: 999px; box-shadow: none;
  min-height: 50px; padding: 0 30px; font-size: 16px; font-weight: 600;
}}
button.oobe-primary:hover {{ background-color: {phover}; }}
button.oobe-primary:disabled {{ background-color: {off}; color: {pfg}; }}
button.oobe-primary.ai-danger {{ background-color: {errbg}; color: #ffffff; }}
button.oobe-primary.ai-danger:hover {{ background-color: {errhover}; }}
button.oobe-back {{
  background: none; background-color: transparent; color: {fg};
  border: none; box-shadow: none; min-height: 50px; padding: 0 16px; font-size: 16px; border-radius: 999px;
}}
button.oobe-back:hover {{ background-color: {hover}; }}
button.oobe-back:disabled {{ color: {faint}; opacity: 0.5; }}

checkbutton.oobe-tool {{ padding: 8px 0; color: {fg}; font-size: 16px; }}
checkbutton.oobe-tool check {{
  background: none; background-color: {bg}; border: 1.5px solid {faint}; border-radius: 4px;
  min-width: 18px; min-height: 18px; box-shadow: none; color: {pfg};
}}
checkbutton.oobe-tool check:checked {{ background-color: {fg}; border-color: {fg}; }}

/* Install choices: one card per option, the chosen one outlined in the text colour. */
button.ai-card {{
  background: none; background-color: transparent; color: {fg};
  border: 1.5px solid {line}; border-radius: 14px; box-shadow: none; padding: 16px 20px;
}}
button.ai-card:hover {{ background-color: {hover}; }}
button.ai-card:checked {{ background-color: transparent; border: 2px solid {fg}; padding: 15px 19px; }}
button.ai-card.ai-danger:checked {{ border-color: {err}; }}
button.ai-card:focus-visible {{ outline: 2px solid {faint}; outline-offset: 2px; }}
box.ai-refused {{ border: 1.5px dashed {fieldline}; border-radius: 14px; padding: 16px 20px; }}
.ai-card-disk {{ font-size: 14px; color: {faint}; }}
button.ai-card label {{ font-weight: 400; }}
button.ai-card label.ai-card-title {{ font-size: 17px; font-weight: 600; color: {fg}; }}
.ai-refused label.ai-card-title {{ font-size: 17px; font-weight: 600; color: {muted}; }}
.ai-card-sub {{ font-size: 15px; color: {muted}; }}
.ai-card-why {{ font-size: 15px; color: {fg}; }}
.ai-card-warn {{ font-size: 14px; color: {warn}; }}
.ai-section {{ font-size: 14px; font-weight: 600; color: {muted}; }}
.ai-help {{ font-size: 15px; color: {muted}; }}

/* Review lines: a dot says what happens to each part of the disk. */
.ai-line {{ font-size: 16px; color: {fg}; }}
.ai-dot {{ min-width: 9px; min-height: 9px; border-radius: 5px; background-color: {line}; }}
.ai-dot.create {{ background-color: {ok}; }}
.ai-dot.erase {{ background-color: {err}; }}
.ai-dot.wait {{ background-color: transparent; border: 1.5px solid {fieldline}; min-width: 7px; min-height: 7px; }}
.ai-time {{ font-size: 15px; color: {faint}; font-feature-settings: \"tnum\"; }}
.ai-message {{ font-size: 13px; color: {fg}; background-color: {hover}; border-radius: 10px; padding: 14px 18px; }}
.ai-disk {{ font-size: 19px; font-weight: 600; color: {fg}; }}
.ai-warnbox {{ background-color: {warnbg}; border-radius: 10px; padding: 14px 18px; }}
.ai-warnbox label {{ font-size: 15px; color: {fg}; }}
.ai-ok {{ font-size: 16px; color: {ok}; font-weight: 600; }}

.ai-key {{ font-family: 'JetBrains Mono', 'Noto Sans Mono', monospace; font-size: 20px; color: {fg}; background-color: {hover}; border-radius: 10px; padding: 16px 20px; letter-spacing: 1px; }}
.ai-log {{ font-family: 'JetBrains Mono', 'Noto Sans Mono', monospace; font-size: 12px; color: {faint}; }}
expander.ai-adv title {{ font-size: 15px; color: {muted}; min-height: 32px; padding: 4px 0; }}
expander.ai-adv arrow {{ color: {muted}; }}
spinner {{ color: {muted}; }}
progressbar trough {{ background-color: {line}; min-height: 4px; border-radius: 2px; }}
progressbar progress {{ background-color: {fg}; min-height: 4px; border-radius: 2px; }}
"
    )
}
