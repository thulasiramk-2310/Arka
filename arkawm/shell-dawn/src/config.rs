//! Settings: colours, wallpaper, texts, the apps on the ring and the commands behind the menu.
//!
//! Four layers, later ones winning: the built-in defaults (`defaults.conf`, which also documents
//! every key), the theme's `theme.conf`, a theme of the same name in the user's folder, and the
//! user's own `~/.config/arkawm/desk.conf`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DEFAULTS: &str = include_str!("defaults.conf");
const FALLBACK_THEME: &str = "sky-blue";

pub struct Config {
    values: HashMap<String, String>,
    pub theme: String,
    /// Where pictures and icons are looked for, first match wins.
    dirs: Vec<PathBuf>,
}

/// Read `key = value` lines into `section.key` entries. `#` starts a comment line.
pub fn parse(text: &str, into: &mut HashMap<String, String>) {
    let mut section = String::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = format!("{}.", name.trim());
        } else if let Some((key, value)) = line.split_once('=') {
            into.insert(format!("{section}{}", key.trim()), value.trim().replace("\\n", "\n"));
        }
    }
}

/// `#RRGGBB` as three numbers from 0 to 1.
pub fn rgb(color: &str) -> Option<(f64, f64, f64)> {
    let hex = color.trim().strip_prefix('#').filter(|hex| hex.len() == 6)?;
    let part = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok().map(|v| v as f64 / 255.0);
    Some((part(0)?, part(2)?, part(4)?))
}

fn user_dir() -> Option<PathBuf> {
    Some(Path::new(&std::env::var_os("HOME")?).join(".config/arkawm"))
}

fn builtin_dir() -> PathBuf {
    [
        std::env::var_os("ARKA_DESK_ASSETS").map(PathBuf::from),
        Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"))),
        Some(PathBuf::from("/usr/share/arka-desk")),
    ]
    .into_iter()
    .flatten()
    .find(|dir| dir.join(FALLBACK_THEME).is_dir())
    .unwrap_or_default()
}

pub fn is_picture(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp"))
}

fn read_into(path: PathBuf, into: &mut HashMap<String, String>) {
    if let Ok(text) = std::fs::read_to_string(path) {
        parse(&text, into);
    }
}

impl Config {
    pub fn load() -> Self {
        let mut values = HashMap::new();
        parse(DEFAULTS, &mut values);
        let user = user_dir();
        let mut mine = HashMap::new();
        if let Some(user) = &user {
            read_into(user.join("desk.conf"), &mut mine);
        }
        // The theme: forced for a test, else the one picked in quick settings, else desk.conf.
        let picked = user.as_ref().and_then(|dir| std::fs::read_to_string(dir.join("theme")).ok());
        let wanted = std::env::var("ARKA_THEME").ok().or(picked).or_else(|| mine.get("theme").cloned());
        let wanted = wanted.map(|name| name.trim().to_string()).filter(|name| !name.is_empty() && !name.contains('/'));
        let builtin = builtin_dir();
        let user_theme = |name: &str| user.as_ref().map(|dir| dir.join("themes").join(name));
        let exists = |name: &str| builtin.join(name).is_dir() || user_theme(name).is_some_and(|dir| dir.is_dir());
        let theme = wanted.filter(|name| exists(name)).unwrap_or_else(|| FALLBACK_THEME.to_string());

        let mut dirs: Vec<PathBuf> = user_theme(&theme).into_iter().collect();
        dirs.push(builtin.join(&theme));
        dirs.push(builtin.join(FALLBACK_THEME));
        for dir in dirs.iter().rev().skip(1) {
            read_into(dir.join("theme.conf"), &mut values);
        }
        values.extend(mine);
        // A picture picked in quick settings wins over the theme's own and over desk.conf.
        let picked = user.as_ref().and_then(|dir| std::fs::read_to_string(dir.join("wallpaper")).ok());
        if let Some(path) = picked.map(|p| p.trim().to_string()).filter(|p| !p.is_empty() && Path::new(p).is_file()) {
            values.insert("wallpaper.file".into(), path);
        }
        Self { values, theme, dirs }
    }

    pub fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or_default()
    }

    /// A number; a missing or unreadable one counts as `otherwise`.
    pub fn number(&self, key: &str, otherwise: f64) -> f64 {
        self.get(key).parse().ok().filter(|v: &f64| v.is_finite()).unwrap_or(otherwise)
    }

    pub fn on(&self, key: &str) -> bool {
        !matches!(self.get(key), "off" | "no" | "false" | "0")
    }

    pub fn color(&self, key: &str) -> (f64, f64, f64) {
        rgb(self.get(&format!("colors.{key}"))).unwrap_or((0.9, 0.93, 0.98))
    }

    /// The commands under this key, in the order to try them.
    pub fn commands(&self, key: &str) -> Vec<String> {
        split_commands(self.get(&format!("commands.{key}")))
    }

    /// A picture or icon of the theme: the user's copy if there is one, else the built-in one.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dirs.iter().map(|dir| dir.join(name)).find(|path| path.exists()).unwrap_or_else(|| self.dirs[0].join(name))
    }

    /// The pictures offered in quick settings: the theme's own wallpaper first, then the others
    /// that come with it (its `wallpapers` folder, the user's copy of the theme first).
    pub fn wallpapers(&self) -> Vec<PathBuf> {
        let mut out = vec![self.file("wallpaper.jpg")];
        if let Some(dir) = self.dirs.iter().map(|dir| dir.join("wallpapers")).find(|dir| dir.is_dir()) {
            let mut more: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|entries| entries.flatten().map(|entry| entry.path()).filter(|p| is_picture(p)).collect())
                .unwrap_or_default();
            more.sort();
            out.extend(more);
        }
        out
    }

    /// The colour names for the style sheet.
    pub fn css_colors(&self) -> String {
        let mut out = String::new();
        for name in ["background", "panel", "panel_raised", "border", "text", "text_dim", "accent", "accent_muted", "ok", "ok_muted", "warn"] {
            let (r, g, b) = self.color(name);
            out.push_str(&format!("@define-color {name} rgb({:.0},{:.0},{:.0});\n", r * 255.0, g * 255.0, b * 255.0));
        }
        out
    }

    /// The apps on the ring: name, icon, commands, highlighted.
    pub fn apps(&self) -> Vec<(String, String, Vec<String>, bool)> {
        (1..=8).filter_map(|place| app(self.get(&format!("apps.{place}")))).collect()
    }
}

pub fn split_commands(list: &str) -> Vec<String> {
    list.split(';').map(str::trim).filter(|command| !command.is_empty()).map(str::to_string).collect()
}

fn app(line: &str) -> Option<(String, String, Vec<String>, bool)> {
    let mut parts = line.split('|').map(str::trim);
    let (name, icon, commands) = (parts.next()?, parts.next()?, parts.next()?);
    (!name.is_empty()).then(|| (name.to_string(), icon.to_string(), split_commands(commands), parts.next() == Some("highlight")))
}

/// Every theme there is: built-in ones and the user's, with the name to show.
pub fn themes() -> Vec<(String, String)> {
    let mut names: Vec<String> = Vec::new();
    for dir in [Some(builtin_dir()), user_dir().map(|dir| dir.join("themes"))].into_iter().flatten() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.sort();
    names.into_iter().map(|name| (title(&name), name)).map(|(title, name)| (name, title)).collect()
}

/// `sky-blue` as `Sky Blue`.
fn title(name: &str) -> String {
    name.split(['-', '_'])
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map(|first| first.to_uppercase().chain(letters).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fill in the two strengths the user can set: `glow(0.3)` is the accent at that strength times
/// the glow setting, `opaque(0.8)` that number times the panel setting.
pub fn apply_strengths(css: &str, glow: f64, panel: f64) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    loop {
        let found = [("glow(", true), ("opaque(", false)]
            .into_iter()
            .filter_map(|(word, is_glow)| rest.find(word).map(|at| (at, word, is_glow)))
            .min_by_key(|(at, _, _)| *at);
        let Some((at, word, is_glow)) = found else { break };
        let after = &rest[at + word.len()..];
        let Some(end) = after.find(')') else { break };
        let value: f64 = after[..end].trim().parse().unwrap_or(0.0);
        out.push_str(&rest[..at]);
        if is_glow {
            out.push_str(&format!("alpha(@accent,{:.3})", (value * glow).clamp(0.0, 1.0)));
        } else {
            out.push_str(&format!("{:.3}", (value * panel).clamp(0.0, 1.0)));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_read() {
        let mut values = HashMap::new();
        parse("theme = emerald\n# note\n[text]\nleft = A\\nB\nempty =\n[apps]\n1 = Web | globe | firefox; brave | highlight", &mut values);
        assert_eq!(values["theme"], "emerald");
        assert_eq!(values["text.left"], "A\nB");
        assert_eq!(values["text.empty"], "");
        assert_eq!(app(&values["apps.1"]), Some(("Web".into(), "globe".into(), vec!["firefox".into(), "brave".into()], true)));
        assert_eq!(app("only a name"), None);
    }

    #[test]
    fn the_defaults_are_complete() {
        let mut values = HashMap::new();
        parse(DEFAULTS, &mut values);
        for name in ["background", "panel", "panel_raised", "border", "text", "text_dim", "accent", "accent_muted", "ok", "ok_muted", "warn"] {
            assert!(rgb(&values[&format!("colors.{name}")]).is_some(), "{name}");
        }
        assert_eq!((1..=8).filter(|place| app(&values[&format!("apps.{place}")]).is_some()).count(), 8);
    }

    #[test]
    fn colours_and_strengths() {
        assert_eq!(rgb("#FF0080"), Some((1.0, 0.0, 128.0 / 255.0)));
        assert_eq!(rgb("red"), None);
        assert_eq!(apply_strengths("a{b:0 0 4px glow(0.5);c:alpha(@panel,opaque(0.8))}", 0.5, 2.0), "a{b:0 0 4px alpha(@accent,0.250);c:alpha(@panel,1.000)}");
        assert_eq!(title("sky-blue"), "Sky Blue");
    }
}
