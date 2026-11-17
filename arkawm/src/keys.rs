//! Shortcuts the user adds, in `~/.config/arkawm/shortcuts`, one per line:
//!
//! ```text
//! # Super is the Windows key. Key names are xkb names: b, Return, F5, Print, slash, ...
//! Super+B = firefox
//! Ctrl+Alt+K = kate ~/notes.txt
//! ```
//!
//! The command runs through `sh -c`. A shortcut here wins over ArkaWM's own one on the same keys.
//! The file is read again whenever it changes, so a saved edit works on the next key press. Like
//! every shortcut, these do nothing while the screen is locked.

use smithay::input::keyboard::{xkb, Keysym, ModifiersState};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    logo: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    key: u32,
    pub command: String,
}

#[derive(Default)]
pub struct UserKeys {
    path: Option<PathBuf>,
    stamp: Option<SystemTime>,
    list: Vec<Binding>,
}

/// Letters compare without case: Shift+B arrives as `B`, the file may say `b`.
fn plain(sym: u32) -> u32 {
    if (0x41..=0x5a).contains(&sym) { sym + 0x20 } else { sym }
}

/// One line of the file, or None for comments, blank lines and lines that make no sense.
pub fn parse_line(line: &str) -> Option<Binding> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (keys, command) = line.split_once('=')?;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let mut binding = Binding { logo: false, ctrl: false, alt: false, shift: false, key: 0, command: command.to_string() };
    let parts: Vec<&str> = keys.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (last, mods) = parts.split_last()?;
    for part in mods {
        match part.to_ascii_lowercase().as_str() {
            "super" | "win" | "windows" | "logo" | "meta" => binding.logo = true,
            "ctrl" | "control" => binding.ctrl = true,
            "alt" => binding.alt = true,
            "shift" => binding.shift = true,
            _ => return None,
        }
    }
    let sym = xkb::keysym_from_name(last, xkb::KEYSYM_CASE_INSENSITIVE);
    if sym.raw() == 0 {
        return None;
    }
    binding.key = plain(sym.raw());
    // A shortcut needs a modifier, or a key that does nothing else (F-keys, Print and the like):
    // a bare letter here would stop that letter from ever being typed.
    let plain_key = binding.key < 0x100;
    if plain_key && !(binding.logo || binding.ctrl || binding.alt) {
        return None;
    }
    Some(binding)
}

impl UserKeys {
    pub fn new() -> UserKeys {
        let path = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|dir| dir.join("arkawm/shortcuts"));
        let mut keys = UserKeys { path, ..Default::default() };
        keys.refresh();
        keys
    }

    /// Read the file again if it changed since the last look (or appeared, or went away).
    pub fn refresh(&mut self) {
        let Some(path) = &self.path else { return };
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if stamp == self.stamp {
            return;
        }
        self.stamp = stamp;
        let text = std::fs::read_to_string(path).unwrap_or_default();
        self.list = text.lines().filter_map(parse_line).collect();
        let skipped = text.lines().filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#')).count() - self.list.len();
        tracing::info!("{} shortcuts of your own read from {}", self.list.len(), path.display());
        if skipped > 0 {
            tracing::warn!("{skipped} lines in {} were not understood and are skipped", path.display());
        }
    }

    /// The user's shortcut for these keys, if there is one.
    pub fn find(&self, modifiers: &ModifiersState, sym: Keysym) -> Option<usize> {
        let key = plain(sym.raw());
        self.list.iter().position(|b| {
            b.key == key && b.logo == modifiers.logo && b.ctrl == modifiers.ctrl && b.alt == modifiers.alt && b.shift == modifiers.shift
        })
    }

    pub fn command(&self, index: usize) -> Option<&str> {
        self.list.get(index).map(|b| b.command.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shortcut_lines() {
        let b = parse_line("Super+B = firefox --private-window").unwrap();
        assert!(b.logo && !b.ctrl && !b.alt && !b.shift);
        assert_eq!(b.key, Keysym::b.raw());
        assert_eq!(b.command, "firefox --private-window");
        let b = parse_line("ctrl+alt+Return=konsole").unwrap();
        assert!(b.ctrl && b.alt && b.key == Keysym::Return.raw());
        // letters match without case
        assert_eq!(parse_line("Super+Shift+B = x").unwrap().key, Keysym::b.raw());
        // a key that types nothing may stand alone
        assert!(parse_line("F9 = gnome-calculator").is_some());
    }

    #[test]
    fn skips_what_it_cannot_use() {
        for line in ["", "# Super+B = firefox", "Super+B =", "Super+NoSuchKey = x", "Hyper+B = x", "b = x", "Shift+b = x", "= x"] {
            assert_eq!(parse_line(line), None, "{line:?}");
        }
    }
}
