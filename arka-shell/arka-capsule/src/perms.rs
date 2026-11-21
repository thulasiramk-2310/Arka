//! What a Flathub app can reach, in plain words. The same wording as `arka install`
//! (apps/arka-install), so the window and the terminal never disagree. Input is the
//! metadata `flatpak remote-info --show-metadata` prints.

#[derive(Debug, Clone, PartialEq)]
pub struct Reach {
    pub text: String,
    /// Reaches well beyond the app itself (all files, every device, X11, your keys).
    pub wide: bool,
    pub icon: &'static str,
}

const SHARED: &[(&str, &str, bool, &str)] = &[("network", "Connect to the internet", false, "globe")];
const SOCKETS: &[(&str, &str, bool, &str)] = &[
    ("x11", "Use X11, an older display system that doesn't keep it apart from other X11 apps", true, "warning"),
    ("pulseaudio", "Play sound and use the microphone", false, "speaker-high"),
    ("ssh-auth", "Use your SSH keys", true, "key"),
    ("pcsc", "Use smart cards", false, "identification-card"),
    ("cups", "Print", false, "printer"),
    ("gpg-agent", "Use your GPG keys", true, "key"),
];
const DEVICES: &[(&str, &str, bool, &str)] = &[
    ("all", "Use every device, including cameras and USB devices", true, "usb"),
    ("usb", "Use USB devices", false, "usb"),
    ("input", "Read game controllers and input devices", false, "game-controller"),
    ("kvm", "Run virtual machines", false, "cpu"),
];
const FEATURES: &[(&str, &str, bool, &str)] = &[
    ("bluetooth", "Use Bluetooth", false, "bluetooth"),
    ("devel", "Use developer features that weaken its sandbox", true, "code"),
    ("canbus", "Use the CAN bus", false, "plugs-connected"),
];
const FILES: &[(&str, &str)] = &[
    ("host", "all your files"),
    ("host-os", "the system's files"),
    ("host-etc", "the system's settings in /etc"),
    ("home", "your whole home folder"),
    ("xdg-download", "your Downloads folder"),
    ("xdg-documents", "your Documents folder"),
    ("xdg-pictures", "your Pictures folder"),
    ("xdg-music", "your Music folder"),
    ("xdg-videos", "your Videos folder"),
    ("xdg-desktop", "your Desktop folder"),
    ("xdg-config", "your settings folder (~/.config)"),
];
const WIDE_FILES: &[&str] = &["host", "host-os", "host-etc", "home", "xdg-config"];

/// Shown under the list, as `arka install` prints it.
pub const CAVEAT: &str = "These are what the app asks for now. An update can change them, and the app can still ask you through your desktop to open a file or use the camera. They limit what the app reaches; they don't make it safe.";

fn sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            out.push((line[1..line.len() - 1].to_string(), Vec::new()));
        } else if let (Some((k, v)), Some(last)) = (line.split_once('='), out.last_mut()) {
            last.1.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

fn get<'a>(secs: &'a [(String, Vec<(String, String)>)], sec: &str, key: &str) -> Vec<&'a str> {
    secs.iter()
        .filter(|(n, _)| n == sec)
        .flat_map(|(_, kv)| kv.iter().filter(|(k, _)| k == key))
        .flat_map(|(_, v)| v.split(';'))
        .filter(|s| !s.is_empty() && !s.starts_with('!'))
        .collect()
}

/// Wide items first, then the rest, each in the order Flatpak lists them.
pub fn parse(text: &str) -> Vec<Reach> {
    let secs = sections(text);
    let mut wide = Vec::new();
    let mut narrow = Vec::new();
    let mut push = |r: Reach| if r.wide { wide.push(r) } else { narrow.push(r) };
    for (key, table) in [("shared", SHARED), ("sockets", SOCKETS), ("devices", DEVICES), ("features", FEATURES)] {
        for v in get(&secs, "Context", key) {
            if let Some((_, text, w, icon)) = table.iter().find(|(k, ..)| *k == v) {
                push(Reach { text: text.to_string(), wide: *w, icon });
            }
        }
    }
    let mut files = false;
    for f in get(&secs, "Context", "filesystems") {
        let (path, mode) = f.split_once(':').unwrap_or((f, "rw"));
        let verb = if mode == "ro" { "Read" } else { "Read and change" };
        if let Some((_, what)) = FILES.iter().find(|(k, _)| *k == path) {
            files = true;
            push(Reach { text: format!("{verb} {what}"), wide: WIDE_FILES.contains(&path), icon: "folder" });
        } else if path.starts_with('~') || path.starts_with('/') {
            files = true;
            push(Reach { text: format!("{verb} the folder {path}"), wide: false, icon: "folder" });
        }
    }
    if !files {
        push(Reach {
            text: "Your files: only the ones you choose in its Open and Save windows".into(),
            wide: false,
            icon: "folder",
        });
    }
    for (sec, where_) in [("Session Bus Policy", "desktop"), ("System Bus Policy", "system")] {
        let n = secs.iter().filter(|(n, _)| n == sec)
            .flat_map(|(_, kv)| kv.iter().filter(|(_, v)| v == "talk" || v == "own")).count();
        if n > 0 {
            push(Reach {
                text: format!("Talk to {n} other service{} on the {where_} bus", if n == 1 { "" } else { "s" }),
                wide: false,
                icon: "plugs-connected",
            });
        }
    }
    wide.extend(narrow);
    wide
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNAL: &str = "[Application]\nname=org.signal.Signal\n\n[Context]\nshared=ipc;network;\nsockets=fallback-x11;pulseaudio;wayland;\ndevices=all;\n";

    #[test]
    fn signal_reads_as_flathub_lists_it() {
        let r = parse(SIGNAL);
        let t: Vec<&str> = r.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(t[0], "Use every device, including cameras and USB devices");
        assert!(r[0].wide);
        assert!(t.contains(&"Connect to the internet"));
        assert!(t.contains(&"Play sound and use the microphone"));
        assert!(t.iter().any(|s| s.starts_with("Your files: only")));
        // fallback-x11 is only used without Wayland; arka install doesn't list it either.
        assert!(!t.iter().any(|s| s.contains("X11")));
    }

    #[test]
    fn home_is_wide_and_read_only_says_read() {
        let r = parse("[Context]\nfilesystems=home:ro;xdg-download;!host;~/Games;\n");
        assert_eq!(r[0], Reach { text: "Read your whole home folder".into(), wide: true, icon: "folder" });
        assert!(r.iter().any(|x| x.text == "Read and change your Downloads folder" && !x.wide));
        assert!(r.iter().any(|x| x.text == "Read and change the folder ~/Games"));
        assert!(!r.iter().any(|x| x.text.contains("all your files")));
        assert!(!r.iter().any(|x| x.text.starts_with("Your files: only")));
    }

    #[test]
    fn bus_names_are_counted() {
        let r = parse("[Context]\nshared=network;\n[Session Bus Policy]\norg.a=talk\norg.b=own\norg.c=see\n");
        assert!(r.iter().any(|x| x.text == "Talk to 2 other services on the desktop bus"));
    }

    #[test]
    fn negated_sockets_are_skipped() {
        let r = parse("[Context]\nsockets=!x11;x11;\n");
        assert_eq!(r.iter().filter(|x| x.text.contains("X11")).count(), 1);
    }
}
