pub struct Perm {
    pub label: &'static str,
    pub desc: &'static str,
    pub icon: &'static str,
    pub key: &'static str,
    pub token: &'static str,
    pub allow: &'static str,
    pub deny: &'static str,
}

pub const PERMS: &[Perm] = &[
    Perm { label: "Network", desc: "Share the host network", icon: "network-wireless-symbolic", key: "shared", token: "network", allow: "--share=network", deny: "--unshare=network" },
    Perm { label: "Audio and microphone", desc: "Playback and recording through the audio server", icon: "audio-input-microphone-symbolic", key: "sockets", token: "pulseaudio", allow: "--socket=pulseaudio", deny: "--nosocket=pulseaudio" },
    Perm { label: "Graphics acceleration", desc: "Access graphics devices for rendering", icon: "video-display-symbolic", key: "devices", token: "dri", allow: "--device=dri", deny: "--nodevice=dri" },
    Perm { label: "All devices", desc: "Broad device access, including hardware beyond cameras", icon: "dialog-warning-symbolic", key: "devices", token: "all", allow: "--device=all", deny: "--nodevice=all" },
    Perm { label: "Home folder grant", desc: "Direct home-folder grant; other paths and portals are separate", icon: "user-home-symbolic", key: "filesystems", token: "home", allow: "--filesystem=home", deny: "--nofilesystem=home" },
    Perm { label: "Downloads folder grant", desc: "Direct Downloads grant; broader grants may also cover this folder", icon: "folder-download-symbolic", key: "filesystems", token: "xdg-download", allow: "--filesystem=xdg-download", deny: "--nofilesystem=xdg-download" },
    Perm { label: "Wayland display", desc: "Show windows through Wayland", icon: "video-display-symbolic", key: "sockets", token: "wayland", allow: "--socket=wayland", deny: "--nosocket=wayland" },
];

/// Match complete entries in the Context group, including :ro/:rw/:create.
/// These switches describe direct grants, not every possible access path.
pub fn is_granted(perm: &Perm, text: &str) -> bool {
    let mut context = false;
    let mut result = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') { context = line == "[Context]"; continue; }
        if !context { continue; }
        let Some((key, values)) = line.split_once('=') else { continue; };
        if key.trim() != perm.key { continue; }
        for value in values.split(';').map(str::trim) {
            let denied = value.starts_with('!');
            let token = value.trim_start_matches('!').split(':').next().unwrap_or("");
            if token == perm.token { result = !denied; }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_sections_tokens_modes_and_denials() {
        assert!(is_granted(&PERMS[0], "[Context]\nshared=network;ipc;"));
        assert!(!is_granted(&PERMS[0], "[Context]\nshared=!network;ipc;"));
        assert!(!is_granted(&PERMS[0], "[Session Bus Policy]\nnetwork=talk"));
        assert!(is_granted(&PERMS[4], "[Context]\nfilesystems=home:ro;"));
        assert!(!is_granted(&PERMS[4], "[Context]\nfilesystems=/some/home;host;!home;"));
    }
    #[test]
    fn capabilities_do_not_impersonate_camera_or_microphone_controls() {
        assert_eq!(PERMS[1].allow, "--socket=pulseaudio");
        assert_eq!(PERMS[2].label, "Graphics acceleration");
        assert_eq!(PERMS[3].label, "All devices");
        assert_eq!(PERMS[0].deny, "--unshare=network");
    }
}
