//! Talking to the installer backend (`arka-install`, run as root). The screens never touch a
//! disk themselves: they ask the backend to probe, to plan and, after the review, to apply.
//!
//! The backend is started without a shell. By default it is `/usr/libexec/arka-install`,
//! through `pkexec` when this app is not root (polkit action org.arkaos.install: an
//! administrator password, kept for a few minutes so probing and planning ask only once);
//! ARKA_INSTALL_BACKEND replaces that command (words split on spaces) for previews and tests.

use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

pub const BACKEND: &str = "/usr/libexec/arka-install";
pub const LOG: &str = "/run/arka-install/install.log";
/// The image the live system carries, which bootc installs from.
pub const DEFAULT_IMAGE: &str = "ghcr.io/thulasiramk-2310/arka:latest";

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Disk {
    pub path: String,
    #[serde(default)]
    pub model: Option<String>,
    pub size_bytes: u64,
    #[serde(default)]
    pub partitions: Vec<serde_json::Value>,
    #[serde(default)]
    pub has_windows: bool,
    #[serde(default)]
    pub in_use: bool,
}

impl Disk {
    /// "Samsung SSD 980", or "Disk" when the disk reports no model.
    pub fn name(&self) -> String {
        self.model.clone().map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or_else(|| "Disk".into())
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Row {
    pub label: Option<String>,
    pub raw_size: Option<u64>,
    pub activity: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Plan {
    pub mode: String,
    pub disk: Disk,
    pub encrypt: bool,
    pub rows: Vec<Row>,
    pub entry_label: String,
    pub warnings: Vec<String>,
    pub review: Vec<String>,
}

impl Plan {
    /// Size of the ArkaOS system partition this plan creates.
    pub fn root_size(&self) -> u64 {
        self.rows
            .iter()
            .filter(|r| r.activity == "create" && r.label.as_deref() == Some("arka-root"))
            .filter_map(|r| r.raw_size)
            .sum()
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Outcome {
    pub kept_unchanged: bool,
    pub windows_boot_files_unchanged: Option<bool>,
    pub boot_entry: Option<String>,
}

/// What the backend said: a result, or a refusal / error message meant for the person.
#[derive(Debug, Clone, PartialEq)]
pub enum Said<T> {
    Ok(T),
    Refused(String),
    Failed(String),
}

/// Sizes the way Windows' Disk Management shows them (binary units labelled GB), so the
/// numbers match what the person sees there. Same rule as the backend.
pub fn gb(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else {
        format!("{} MB", bytes / MIB)
    }
}

fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).map(|l| l.split_whitespace().nth(1) == Some("0")))
        .unwrap_or(false)
}

fn command(args: &[&str]) -> Command {
    let words: Vec<String> = match std::env::var("ARKA_INSTALL_BACKEND") {
        Ok(v) if !v.trim().is_empty() => v.split_whitespace().map(String::from).collect(),
        _ if is_root() => vec![BACKEND.into()],
        _ => vec!["pkexec".into(), BACKEND.into()],
    };
    let mut c = Command::new(&words[0]);
    c.args(&words[1..]).args(args);
    c
}

/// Reads the backend's last JSON line as T, or its refusal / error event.
pub fn parse<T: for<'de> Deserialize<'de>>(stdout: &str, ok: bool) -> Said<T> {
    let text = stdout.trim();
    if ok {
        return match serde_json::from_str::<T>(text) {
            Ok(v) => Said::Ok(v),
            Err(e) => Said::Failed(format!("The installer gave an answer this screen could not read ({e}).")),
        };
    }
    let last = text.lines().rev().find(|l| l.trim_start().starts_with('{')).unwrap_or("");
    match serde_json::from_str::<serde_json::Value>(last) {
        Ok(v) => {
            let msg = v["message"].as_str().unwrap_or("The installer stopped without saying why.").to_string();
            if v["event"] == "refused" {
                Said::Refused(msg)
            } else {
                Said::Failed(msg)
            }
        }
        Err(_) => Said::Failed("The installer could not be started.".into()),
    }
}

/// pkexec exits 126 when the password dialog is dismissed or the password is wrong, and 127
/// when this user may not run the installer at all.
const NOT_ALLOWED: &str = "Installing needs an administrator password. It was not given, so nothing was read or written. Go back and continue to be asked again.";

fn denied(code: Option<i32>) -> bool {
    std::env::var("ARKA_INSTALL_BACKEND").map(|v| v.trim().is_empty()).unwrap_or(true) && !is_root() && matches!(code, Some(126 | 127))
}

fn run<T: for<'de> Deserialize<'de>>(args: &[&str]) -> Said<T> {
    match command(args).stdin(Stdio::null()).stderr(Stdio::null()).output() {
        Ok(o) if denied(o.status.code()) => Said::Refused(NOT_ALLOWED.into()),
        Ok(o) => parse(&String::from_utf8_lossy(&o.stdout), o.status.success()),
        Err(e) => Said::Failed(format!("The installer could not be started: {e}.")),
    }
}

pub fn probe() -> Said<Vec<Disk>> {
    run(&["probe"])
}

/// Plans an install; the plan is saved to `out` for apply.
pub fn plan(disk: &str, mode: &str, encrypt: bool, keymap: &str, image: &str, out: &str) -> Said<Plan> {
    let mut args = vec!["plan", "--disk", disk, "--mode", mode, "--image", image, "--keymap", keymap, "--out", out];
    if !encrypt {
        args.push("--no-encrypt");
    }
    run(&args)
}

/// One thing apply reported while it ran.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Step(String),
    /// A line of the backend's own log, for the details view.
    Log(String),
    Done(Outcome),
    Failed(String),
}

impl PartialEq for Outcome {
    fn eq(&self, o: &Self) -> bool {
        self.kept_unchanged == o.kept_unchanged && self.windows_boot_files_unchanged == o.windows_boot_files_unchanged && self.boot_entry == o.boot_entry
    }
}

/// One stdout line of `arka-install apply` as an event.
pub fn event(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    match v["event"].as_str()? {
        "step" => Some(Event::Step(v["step"].as_str()?.to_string())),
        "done" => Some(Event::Done(serde_json::from_value(v["result"].clone()).ok()?)),
        "error" | "refused" => Some(Event::Failed(v["message"].as_str().unwrap_or("The install stopped.").to_string())),
        _ => None,
    }
}

/// Runs apply on a thread, sending its events. The passphrase goes to stdin only.
pub fn apply(plan_file: String, passphrase: Option<String>, tx: std::sync::mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        let child = command(&["apply", "--plan", &plan_file]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Event::Failed(format!("The installer could not be started: {e}.")));
                return;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Some(p) = passphrase {
                let _ = stdin.write_all(p.as_bytes());
                let _ = stdin.write_all(b"\n");
            }
        }
        if let Some(err) = child.stderr.take() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    let _ = tx.send(Event::Log(line));
                }
            });
        }
        let mut finished = false;
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(e) = event(&line) {
                    finished |= matches!(e, Event::Done(_) | Event::Failed(_));
                    let _ = tx.send(e);
                }
            }
        }
        let status = child.wait();
        if !finished && denied(status.as_ref().ok().and_then(|s| s.code())) {
            let _ = tx.send(Event::Failed(NOT_ALLOWED.into()));
        } else if !finished {
            let code = status.ok().and_then(|s| s.code()).map(|c| format!(" (exit {c})")).unwrap_or_default();
            let _ = tx.send(Event::Failed(format!("The installer stopped without a result{code}.")));
        }
    });
}

/// The keyboard layouts the console knows (`localectl list-keymaps`), or a short fallback.
pub fn keymaps() -> Vec<String> {
    let listed = Command::new("localectl")
        .arg("list-keymaps")
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    if listed.is_empty() {
        ["us", "uk", "de", "fr", "es", "it", "dvorak", "colemak"].iter().map(|s| s.to_string()).collect()
    } else {
        listed
    }
}

/// True when the computer has a battery and no charger is connected (read from
/// /sys/class/power_supply). A desktop without a battery is never "on battery".
/// ARKA_INSTALL_POWER=battery|mains overrides it for previews.
pub fn on_battery() -> bool {
    match std::env::var("ARKA_INSTALL_POWER").as_deref() {
        Ok("battery") => return true,
        Ok("mains") => return false,
        _ => {}
    }
    let read = |p: &std::path::Path, f: &str| std::fs::read_to_string(p.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
    let (mut battery, mut charger) = (false, false);
    if let Ok(dir) = std::fs::read_dir("/sys/class/power_supply") {
        for e in dir.flatten() {
            let p = e.path();
            match read(&p, "type").as_str() {
                "Battery" if read(&p, "scope") != "Device" => battery = true,
                "Mains" | "USB" if read(&p, "online") == "1" => charger = true,
                _ => {}
            }
        }
    }
    battery && !charger
}

/// A readable name for a console keymap: "English (US) · us". Layouts not listed keep their code.
pub fn keymap_name(code: &str) -> String {
    const NAMES: &[(&str, &str)] = &[
        ("us", "English (US)"),
        ("uk", "English (UK)"),
        ("us-acentos", "English (US, international)"),
        ("de", "German"),
        ("de-latin1-nodeadkeys", "German, no dead keys"),
        ("fr", "French"),
        ("fr-latin1", "French (Latin-1)"),
        ("be-latin1", "Belgian"),
        ("es", "Spanish"),
        ("it", "Italian"),
        ("pt-latin1", "Portuguese"),
        ("br-abnt2", "Portuguese (Brazil)"),
        ("nl", "Dutch"),
        ("se-lat6", "Swedish"),
        ("no", "Norwegian"),
        ("dk", "Danish"),
        ("fi", "Finnish"),
        ("pl2", "Polish"),
        ("cz-qwertz", "Czech"),
        ("hu", "Hungarian"),
        ("ru", "Russian"),
        ("ua", "Ukrainian"),
        ("gr", "Greek"),
        ("trq", "Turkish (Q)"),
        ("jp106", "Japanese"),
        ("ch", "Swiss German"),
        ("fr_CH", "Swiss French"),
        ("dvorak", "English (Dvorak)"),
        ("colemak", "English (Colemak)"),
    ];
    match NAMES.iter().find(|(c, _)| *c == code) {
        Some((_, n)) => format!("{n} · {code}"),
        None => code.to_string(),
    }
}

/// One choice on the "Where to install" page.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub disk: String,
    pub mode: &'static str,
    pub title: String,
    /// "Samsung SSD 980 · 476.9 GB · /dev/nvme0n1"
    pub disk_line: String,
    pub sub: String,
    /// Why this choice can't be used; shown greyed out with the reason.
    pub refused: Option<String>,
    pub warnings: Vec<String>,
    pub destructive: bool,
}

/// The choices for one disk, from the probe and the backend's answer for each mode.
/// A disk that holds the running system gives none. Refused modes are dropped, except
/// "next to Windows", whose refusal says how to make room.
pub fn choices(d: &Disk, free: &Said<Plan>, empty: &Said<Plan>, erase: &Said<Plan>) -> Vec<Choice> {
    if d.in_use {
        return vec![];
    }
    let disk_line = format!("{} · {} · {}", d.name(), gb(d.size_bytes), d.path);
    let warnings = |s: &Said<Plan>| match s {
        Said::Ok(p) => p.warnings.clone(),
        _ => vec![],
    };
    let mut out = vec![];
    let besides = if d.has_windows { "Next to Windows" } else { "In the free space on this disk" };
    match free {
        Said::Ok(p) => out.push(Choice {
            disk: d.path.clone(),
            mode: "free",
            title: besides.into(),
            disk_line: disk_line.clone(),
            sub: format!(
                "ArkaOS gets {} of free space. The partitions already on this disk are kept as they are.",
                gb(p.root_size() + (2 * GIB))
            ),
            refused: None,
            warnings: warnings(free),
            destructive: false,
        }),
        Said::Refused(why) if d.has_windows => out.push(Choice {
            disk: d.path.clone(),
            mode: "free",
            title: besides.into(),
            disk_line: disk_line.clone(),
            sub: String::new(),
            refused: Some(why.clone()),
            warnings: vec![],
            destructive: false,
        }),
        _ => {}
    }
    if let Said::Ok(_) = empty {
        out.push(Choice {
            disk: d.path.clone(),
            mode: "empty",
            title: "Use this empty disk".into(),
            disk_line: disk_line.clone(),
            sub: "The whole disk is used for ArkaOS.".into(),
            refused: None,
            warnings: warnings(empty),
            destructive: false,
        });
    }
    if let (Said::Ok(_), false) = (erase, d.partitions.is_empty()) {
        out.push(Choice {
            disk: d.path.clone(),
            mode: "erase",
            title: "Erase this disk".into(),
            disk_line: disk_line.clone(),
            // The backend's warning names Windows when it is on the disk.
            sub: "Deletes everything on this disk, then installs ArkaOS on all of it.".into(),
            refused: None,
            warnings: warnings(erase),
            destructive: true,
        });
    }
    out
}

/// Passphrase rules: at least 8 characters, typed the same twice. None when it is fine.
pub fn passphrase_problem(p: &str, again: &str) -> Option<&'static str> {
    if p.chars().count() < 8 {
        Some("Use at least 8 characters.")
    } else if p != again {
        Some("The two passphrases are different.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(windows: bool, parts: usize, _gap: u64) -> Disk {
        Disk {
            path: "/dev/vdb".into(),
            model: Some("QEMU disk ".into()),
            size_bytes: 80 * GIB,
            partitions: vec![serde_json::Value::Null; parts],
            has_windows: windows,
            ..Default::default()
        }
    }
    fn plan(root: u64) -> Said<Plan> {
        Said::Ok(Plan {
            mode: "free".into(),
            disk: disk(true, 4, 0),
            encrypt: true,
            rows: vec![
                Row { label: Some("arka-boot".into()), raw_size: Some(2 * GIB), activity: "create".into() },
                Row { label: Some("arka-root".into()), raw_size: Some(root), activity: "create".into() },
                Row { label: Some("Basic data partition".into()), raw_size: Some(24 * GIB), activity: "unchanged".into() },
            ],
            entry_label: "ArkaOS".into(),
            warnings: vec![],
            review: vec![],
        })
    }

    #[test]
    fn windows_disk_offers_free_space_and_erase() {
        let c = choices(&disk(true, 4, 50 * GIB), &plan(48 * GIB), &Said::Refused("has partitions".into()), &plan(77 * GIB));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].title, "Next to Windows");
        assert_eq!(c[0].disk_line, "QEMU disk · 80.0 GB · /dev/vdb");
        assert!(c[0].sub.starts_with("ArkaOS gets 50.0 GB"));
        assert!(c[1].destructive);
    }

    #[test]
    fn too_little_room_next_to_windows_explains_itself() {
        let why = "The largest free area is 16.7 GB; ArkaOS needs 40.0 GB.".to_string();
        let c = choices(&disk(true, 4, 16 * GIB), &Said::Refused(why.clone()), &Said::Refused("x".into()), &plan(77 * GIB));
        assert_eq!(c[0].refused.as_deref(), Some(why.as_str()));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn empty_disk_offers_only_empty() {
        let c = choices(&disk(false, 0, 0), &Said::Refused("no gpt".into()), &plan(77 * GIB), &plan(77 * GIB));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].mode, "empty");
    }

    #[test]
    fn running_system_disk_offers_nothing() {
        let mut d = disk(true, 4, 50 * GIB);
        d.in_use = true;
        assert!(choices(&d, &plan(48 * GIB), &plan(1), &plan(1)).is_empty());
    }

    #[test]
    fn backend_answers_are_read() {
        assert_eq!(
            parse::<Vec<Disk>>("{\"event\":\"refused\",\"message\":\"This disk has partitions on it.\"}", false),
            Said::Refused("This disk has partitions on it.".into())
        );
        assert!(matches!(parse::<Vec<Disk>>("not json", false), Said::Failed(_)));
        assert_eq!(event("{\"event\":\"step\",\"step\":\"install\"}"), Some(Event::Step("install".into())));
        let done = event("{\"event\":\"done\",\"result\":{\"boot_entry\":\"Boot0005 ArkaOS\",\"created\":[],\"kept_unchanged\":true,\"log\":\"x\",\"windows_boot_files_unchanged\":true}}");
        assert!(matches!(done, Some(Event::Done(o)) if o.kept_unchanged && o.windows_boot_files_unchanged == Some(true)));
    }

    #[test]
    fn keymap_names() {
        assert_eq!(keymap_name("us"), "English (US) · us");
        assert_eq!(keymap_name("de-latin1"), "de-latin1");
    }

    #[test]
    fn passphrases() {
        assert!(passphrase_problem("short", "short").is_some());
        assert!(passphrase_problem("long enough", "long enougH").is_some());
        assert_eq!(passphrase_problem("long enough", "long enough"), None);
    }
}

impl PartialEq for Plan {
    fn eq(&self, o: &Self) -> bool {
        self.review == o.review && self.mode == o.mode
    }
}
impl PartialEq for Disk {
    fn eq(&self, o: &Self) -> bool {
        self.path == o.path
    }
}
