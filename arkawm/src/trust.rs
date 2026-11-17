//! Which programs may use the powerful protocols.
//!
//! An input method sees every key typed, a virtual keyboard can type for the user, and a locker
//! decides when the screen unlocks. ArkaWM offers those three only to programs on a short list,
//! matched by the path of the program file behind the connection (`/proc/<pid>/exe`).
//!
//! Limits, stated plainly: this stops an ordinary app from registering as a keyboard or locker. It
//! does not stop a program that can already tamper with a trusted one running as the same user
//! (for example by injecting code into it); that needs sandboxing of the untrusted app, not this.

use std::path::{Path, PathBuf};

use smithay::reexports::wayland_server::Client;

#[derive(Clone, Copy)]
pub enum Role {
    /// Input method and its virtual keyboard.
    Typing,
    /// Screen locker.
    Locker,
    /// Reads and sets the clipboard without having the focus (clipboard history, screenshots).
    Clipboard,
    /// Sees every open window's title and app, and can bring one forward or close it (a dock).
    Windows,
}

const TYPING: &[&str] = &["/usr/bin/fcitx5", "/usr/bin/ibus-daemon", "/usr/libexec/ibus-ui-gtk3"];
const LOCKERS: &[&str] = &["/usr/bin/swaylock", "/usr/bin/arka-lock"];
// Any program can start these two, so this list keeps apps from using the protocol directly but
// does not keep a program of the same user from reading the clipboard through wl-paste.
const CLIPBOARD: &[&str] = &["/usr/bin/wl-paste", "/usr/bin/wl-copy"];
const WINDOWS: &[&str] = &["/usr/bin/crystal-dock"];

/// Extra trusted programs for development: `ARKAWM_TRUST_TYPING` / `ARKAWM_TRUST_LOCKER`,
/// absolute paths separated by `:`.
fn extra(role: Role) -> Vec<PathBuf> {
    let name = match role {
        Role::Typing => "ARKAWM_TRUST_TYPING",
        Role::Locker => "ARKAWM_TRUST_LOCKER",
        Role::Clipboard => "ARKAWM_TRUST_CLIPBOARD",
        Role::Windows => "ARKAWM_TRUST_WINDOWS",
    };
    std::env::var(name)
        .map(|v| v.split(':').filter(|p| p.starts_with('/')).map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// Whether the program at `exe` is on the list for `role`.
pub fn allowed(role: Role, exe: &Path, extra: &[PathBuf]) -> bool {
    let builtin = match role {
        Role::Typing => TYPING,
        Role::Locker => LOCKERS,
        Role::Clipboard => CLIPBOARD,
        Role::Windows => WINDOWS,
    };
    builtin.iter().any(|p| Path::new(p) == exe) || extra.iter().any(|p| p == exe)
}

/// The program file behind a process, read when it connects.
pub fn program_of(pid: i32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// Whether this connection belongs to a program trusted for `role`. Unknown means no.
///
/// Runs inside the display's own callbacks, so it must not ask the display anything: it only
/// reads what was recorded about the client when it connected.
pub fn client_trusted(client: &Client, role: Role) -> bool {
    let Some(exe) = client.get_data::<crate::state::ClientState>().and_then(|data| data.program.get()) else {
        return false;
    };
    allowed(role, exe, &extra(role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_programs_are_trusted() {
        assert!(allowed(Role::Typing, Path::new("/usr/bin/fcitx5"), &[]));
        assert!(allowed(Role::Locker, Path::new("/usr/bin/swaylock"), &[]));
        assert!(allowed(Role::Clipboard, Path::new("/usr/bin/wl-paste"), &[]));
        assert!(!allowed(Role::Clipboard, Path::new("/usr/bin/kitty"), &[]));
        // a role does not carry over
        assert!(!allowed(Role::Locker, Path::new("/usr/bin/fcitx5"), &[]));
        assert!(!allowed(Role::Typing, Path::new("/usr/bin/swaylock"), &[]));
        // same name somewhere else is a different program
        assert!(!allowed(Role::Typing, Path::new("/home/ram/bin/fcitx5"), &[]));
        assert!(!allowed(Role::Typing, Path::new("/usr/bin/kitty"), &[]));
        assert!(allowed(Role::Locker, Path::new("/opt/x/locker"), &[PathBuf::from("/opt/x/locker")]));
    }
}
