//! Window-management abstraction.
//!
//! Arka applications speak only the [`WindowService`] trait — never a specific
//! compositor's IPC. Today the only backend is KWin (Plasma 6); when ArkaWM
//! arrives it implements the same trait and every app keeps working unchanged:
//!
//! ```text
//!     Capsule ─▶ WindowService ─▶ KWin        (today)
//!     Capsule ─▶ WindowService ─▶ KWin/ArkaWM (tomorrow)
//! ```
//!
//! This is deliberately the boundary that lets ArkaOS eventually replace the
//! window manager without rewriting the desktop applications. It replaces the
//! previous direct `hyprctl` coupling that lingered from the Hyprland era.
//!
//! NOTE (DP2): the KWin backend drives KWin's Scripting D-Bus interface. It
//! compiles and is the correct architecture, but its runtime behaviour is to
//! be verified live during the DP2 build cycle — it is not claimed working on
//! a booted system yet.

use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

struct PrivateScript {
    path: PathBuf,
    dir: PathBuf,
}

impl PrivateScript {
    fn create_in(root: &Path, js: &str) -> std::io::Result<Self> {
        let mut nonce = [0u8; 16];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        let suffix: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let dir = root.join(format!("arka-kwin-{suffix}"));
        // Atomic directory creation never accepts a pre-existing attacker path.
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        let script = Self {
            path: dir.join("script.js"),
            dir,
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&script.path)?;
        file.write_all(js.as_bytes())?;
        Ok(script)
    }
}

impl Drop for PrivateScript {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.dir);
    }
}

// KWin internalId is a QUuid. Journal text and window titles are untrusted;
// never interpolate an arbitrary value from them into compositor JavaScript.
fn valid_window_id(id: &str) -> bool {
    let id = if id.starts_with('{') {
        let Some(id) = id.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
            return false;
        };
        id
    } else {
        id
    };
    let parts: Vec<_> = id.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(part, len)| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// A window on the user's desktop, described in compositor-neutral terms.
#[derive(Clone, Debug, Default)]
pub struct Window {
    /// Opaque handle the active backend understands (KWin `internalId`).
    pub id: String,
    /// Human-readable window caption.
    pub title: String,
    /// Wayland `app-id` / X11 resource class.
    pub app_id: String,
}

/// Window management, abstracted from any single compositor.
///
/// Callers never hardcode a window manager; they obtain an implementation via
/// [`window_service`] and speak only this vocabulary.
pub trait WindowService {
    /// Windows currently open on the desktop (Arka shell chrome excluded).
    fn list(&self) -> Vec<Window>;
    /// Bring the window with `id` to the foreground.
    fn focus(&self, id: &str);
    /// Ask the window with `id` to close.
    fn close(&self, id: &str);
}

/// The window service for the running desktop, chosen at runtime.
///
/// KWin today; a future ArkaWM backend would be selected here (e.g. by probing
/// the running compositor) without any caller changing.
pub fn window_service() -> Box<dyn WindowService> {
    Box::new(KWinWindowService)
}

/// KWin (Plasma) backend, driven through KWin's Scripting D-Bus interface.
///
/// `focus`/`close` load a one-shot KWin script that targets the window by its
/// `internalId`. `list` runs a script that prints a marker line per window,
/// read back from the user journal. `dbus-send` is used (universally present)
/// rather than the version-variable `qdbus`/`qdbus6`.
pub struct KWinWindowService;

/// Marker prefix the enumeration script prints so we can pick our lines out of
/// the journal without depending on KWin's log identifier.
const MARK: &str = "ARKA_WIN\t";

impl KWinWindowService {
    /// Write `js` to a temp file, load+run it via KWin's Scripting D-Bus API,
    /// then unload. Best-effort: any failure is swallowed (returns `false`).
    fn run_script(js: &str) -> bool {
        let Ok(prepared) = PrivateScript::create_in(&std::env::temp_dir(), js) else {
            return false;
        };
        let out = Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--reply-timeout=3000",
                "--dest=org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting.loadScript",
            ])
            .arg(format!("string:{}", prepared.path.display()))
            .output();

        let ok = match out {
            Ok(o) if o.status.success() => {
                // reply tail: "   int32 <id>"
                let reply = String::from_utf8_lossy(&o.stdout);
                if let Some(id) = reply.split_whitespace().last() {
                    let script = format!("/Scripting/Script{id}");
                    let run = Command::new("dbus-send")
                        .args([
                            "--session",
                            "--print-reply",
                            "--reply-timeout=3000",
                            "--dest=org.kde.KWin",
                            &script,
                            "org.kde.kwin.Script.run",
                        ])
                        .output();
                    let _ = Command::new("dbus-send")
                        .args([
                            "--session",
                            "--print-reply",
                            "--reply-timeout=3000",
                            "--dest=org.kde.KWin",
                            &script,
                            "org.kde.kwin.Script.stop",
                        ])
                        .output();
                    run.is_ok_and(|out| out.status.success())
                } else {
                    false
                }
            }
            _ => false,
        };
        ok
    }
}

impl WindowService for KWinWindowService {
    fn list(&self) -> Vec<Window> {
        // Enumerate windows, printing one marker line each. Works on Plasma 6
        // (`windowList`) and falls back to Plasma 5 (`clientList`). Arka shell
        // chrome is filtered out by resourceClass.
        let js = format!(
            r#"
            var wins = (typeof workspace.windowList === "function")
                ? workspace.windowList() : workspace.clientList();
            wins.forEach(function(w) {{
                if (w.skipTaskbar || w.desktopWindow || w.dock) return;
                var cls = (w.resourceClass || "").toString();
                if (cls.indexOf("arka.") === 0) return;
                print("{MARK}" + w.internalId + "\t" + cls + "\t" + (w.caption || ""));
            }});
        "#
        );
        if !Self::run_script(&js) {
            return Vec::new();
        }
        // KWin runs the script asynchronously; read the last couple of seconds
        // of the user journal and pick out our marker lines.
        let out = Command::new("journalctl")
            .args(["--user", "--since", "-3s", "-o", "cat", "--no-pager"])
            .output();
        let text = out
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        let mut windows = Vec::new();
        for line in text.lines().rev() {
            if let Some(rest) = line.split_once(MARK).map(|(_, r)| r) {
                let mut f = rest.splitn(3, '\t');
                let id = f.next().unwrap_or("").to_string();
                let app_id = f.next().unwrap_or("").to_string();
                let title = f.next().unwrap_or("").to_string();
                if !valid_window_id(&id) {
                    continue;
                }
                if windows.iter().any(|w: &Window| w.id == id) {
                    continue; // keep the most recent print per window
                }
                windows.push(Window { id, title, app_id });
            }
        }
        windows.reverse();
        windows
    }

    fn focus(&self, id: &str) {
        if !valid_window_id(id) {
            return;
        }
        let js = format!(
            r#"
            var wins = (typeof workspace.windowList === "function")
                ? workspace.windowList() : workspace.clientList();
            wins.forEach(function(w) {{
                if (w.internalId == "{id}") workspace.activeWindow = w;
            }});
        "#
        );
        Self::run_script(&js);
    }

    fn close(&self, id: &str) {
        if !valid_window_id(id) {
            return;
        }
        let js = format!(
            r#"
            var wins = (typeof workspace.windowList === "function")
                ? workspace.windowList() : workspace.clientList();
            wins.forEach(function(w) {{
                if (w.internalId == "{id}") w.closeWindow();
            }});
        "#
        );
        Self::run_script(&js);
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn rejects_journal_script_injection_and_accepts_kwin_uuids() {
        for id in [
            "00112233-4455-6677-8899-aabbccddeeff",
            "{00112233-4455-6677-8899-AABBCCDDEEFF}",
        ] {
            assert!(valid_window_id(id));
        }
        for id in [
            "",
            "1",
            "\"); print(\"injected\"); //",
            "{00112233-4455-6677-8899-aabbccddeeff",
            "00112233-4455-6677-8899-aabbccddeeff\n",
            "00112233-4455-6677-8899-aabbccddeezz",
        ] {
            assert!(!valid_window_id(id), "{id:?}");
        }
    }

    #[test]
    fn scripts_are_private_unique_and_cleaned_up() {
        let first = PrivateScript::create_in(&std::env::temp_dir(), "first").unwrap();
        let second = PrivateScript::create_in(&std::env::temp_dir(), "second").unwrap();
        assert_ne!(first.path, second.path);
        assert_eq!(
            std::fs::metadata(&first.dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_to_string(&first.path).unwrap(), "first");
        let dir = first.dir.clone();
        drop(first);
        assert!(!dir.exists());
        assert!(second.path.exists());
    }
}
