//! Wi-Fi networks and the hotspot, through NetworkManager's `nmcli`.
//!
//! Passwords never go on a command line (any user on the machine can read those): they are
//! handed to nmcli's editor on its standard input.

use std::io::Write;
use std::process::{Command, Stdio};

/// Prints the id of the first Wi-Fi profile set up as a hotspot (access point), if any.
pub const FIND_HOTSPOT: &str = r#"for c in $(nmcli -g UUID,TYPE connection show | sed -n 's/:802-11-wireless$//p'); do [ "$(nmcli -g 802-11-wireless.mode connection show "$c")" = ap ] && { echo "$c"; break; }; done"#;

/// One Wi-Fi network in reach.
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub ssid: String,
    /// 0 to 100.
    pub signal: u8,
    /// As nmcli shows it: "" for an open network, "WPA2", "WPA2 WPA3", "WPA2 802.1X" and so on.
    pub security: String,
    /// The computer is connected to it now.
    pub active: bool,
    /// The saved profile for it, when there is one.
    pub saved: Option<String>,
}

impl Network {
    pub fn open(&self) -> bool {
        self.security.is_empty() || self.security == "--"
    }

    /// Networks that need more than a password (a user name, a certificate) or use the broken
    /// WEP are left to the full network settings.
    pub fn simple(&self) -> bool {
        !self.security.contains("802.1X") && !self.security.contains("WEP")
    }
}

/// Split one line of `nmcli -t` output into its fields (`\:` is a colon inside a field).
pub fn fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    fields.last_mut().unwrap().push(next);
                }
            }
            ':' => fields.push(String::new()),
            _ => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

/// The networks in `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list`: one entry per
/// name (the strongest), hidden ones left out, the connected one first, then by signal.
pub fn parse_networks(list: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in list.lines() {
        let f = fields(line);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let network = Network {
            ssid: f[1].clone(),
            signal: f[2].parse().unwrap_or(0),
            security: f[3].clone(),
            active: f[0] == "*",
            saved: None,
        };
        match networks.iter_mut().find(|old| old.ssid == network.ssid) {
            Some(old) => {
                old.active |= network.active;
                old.signal = old.signal.max(network.signal);
            }
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    networks.truncate(20);
    networks
}

/// Run a command with a time limit; its output when it worked, else its error message.
fn run(seconds: u32, program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("timeout")
        .arg(seconds.to_string())
        .arg(program)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(error_text(&String::from_utf8_lossy(&out.stderr)))
    }
}

/// nmcli's own message, without its "Error: " prefix; a plain one when it said nothing.
fn error_text(stderr: &str) -> String {
    let line = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).last().unwrap_or("");
    let line = line.strip_prefix("Error: ").unwrap_or(line);
    if line.is_empty() { "NetworkManager did not answer in time.".into() } else { line.to_string() }
}

fn sh(seconds: u32, script: &str) -> Result<String, String> {
    run(seconds, "sh", &["-c", script])
}

/// The hotspot profile's id and whether it is running.
pub fn hotspot() -> Option<(String, bool)> {
    let id = sh(8, FIND_HOTSPOT).ok().filter(|id| !id.is_empty())?;
    let state = run(4, "nmcli", &["-g", "GENERAL.STATE", "connection", "show", &id]).unwrap_or_default();
    Some((id, state.starts_with("activat")))
}

/// The networks in reach, with their saved profiles. Rescanning takes a few seconds.
pub fn networks(rescan: bool) -> Result<Vec<Network>, String> {
    if run(4, "nmcli", &["radio", "wifi"]).as_deref() != Ok("enabled") {
        return Err("Wi-Fi is off.".into());
    }
    if hotspot().is_some_and(|(_, on)| on) {
        return Err("The Wi-Fi card is running your hotspot, so it can't look for networks. Turn the hotspot off to see them.".into());
    }
    let list = run(
        20,
        "nmcli",
        &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", if rescan { "yes" } else { "auto" }],
    )?;
    let mut found = parse_networks(&list);
    // which of them have a saved profile (hotspot profiles are not for joining)
    let saved = sh(
        15,
        r#"for c in $(nmcli -g UUID,TYPE connection show | sed -n 's/:802-11-wireless$//p'); do printf '%s\t%s\t%s\n' "$c" "$(nmcli -g 802-11-wireless.mode connection show "$c")" "$(nmcli -g 802-11-wireless.ssid connection show "$c")"; done"#,
    )
    .unwrap_or_default();
    for line in saved.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(id), Some(mode), Some(ssid)) = (parts.next(), parts.next(), parts.next()) else { continue };
        if mode == "ap" {
            continue;
        }
        if let Some(network) = found.iter_mut().find(|network| network.ssid == ssid && network.saved.is_none()) {
            network.saved = Some(id.to_string());
        }
    }
    if found.is_empty() {
        Err("No networks found.".into())
    } else {
        Ok(found)
    }
}

/// A value for nmcli's editor: one line, no characters that end it early.
fn line_safe(value: &str) -> bool {
    !value.contains(['\n', '\r'])
}

/// Write `commands` to nmcli's editor (`nmcli connection edit ARGS`); the profile's id on success.
fn edit(args: &[&str], commands: &str) -> Result<String, String> {
    let mut child = Command::new("timeout")
        .arg("15")
        .arg("nmcli")
        .args(["connection", "edit"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;
    child.stdin.take().unwrap().write_all(commands.as_bytes()).map_err(|err| err.to_string())?;
    let out = child.wait_with_output().map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "Connection 'x' (UUID) successfully saved." for a new profile, "... updated." for an old one
    text.lines()
        .find(|line| line.contains(") successfully saved") || line.contains(") successfully updated"))
        .and_then(|line| Some(line[line.rfind('(')? + 1..line.rfind(')')?].to_string()))
        .ok_or_else(|| {
            let said = text
                .lines()
                .map(|line| line.trim_start_matches("nmcli> ").trim())
                .filter(|line| line.starts_with("Error") && !line.contains("'yes'"))
                .last();
            said.map(|line| error_text(line)).unwrap_or_else(|| error_text(&String::from_utf8_lossy(&out.stderr)))
        })
}

/// Connect to a network: through its saved profile, or a new one made with `password`. A new
/// profile that does not connect (a wrong password) is removed again.
pub fn join(network: &Network, password: &str) -> Result<(), String> {
    if let (Some(id), true) = (&network.saved, password.is_empty()) {
        return run(45, "nmcli", &["connection", "up", id]).map(|_| ());
    }
    if !line_safe(&network.ssid) || !line_safe(password) {
        return Err("That name or password can't be used here.".into());
    }
    // saved without connecting by itself (else the editor stops to ask); turned on once it works
    let mut commands = format!(
        "set connection.id {ssid}\nset 802-11-wireless.ssid {ssid}\nset connection.autoconnect no\n",
        ssid = network.ssid
    );
    if !network.open() {
        // WPA3-only networks need SAE; mixed WPA2/WPA3 ones take the WPA2 key
        let sae = network.security.contains("WPA3") && !network.security.contains("WPA2") && !network.security.contains("WPA1");
        commands += &format!(
            "set 802-11-wireless-security.key-mgmt {}\nset 802-11-wireless-security.psk {password}\n",
            if sae { "sae" } else { "wpa-psk" }
        );
    }
    commands += "save\nquit\n";
    let id = edit(&["type", "wifi"], &commands)?;
    run(45, "nmcli", &["connection", "up", &id])
        .inspect_err(|_| {
            let _ = run(8, "nmcli", &["connection", "delete", &id]);
        })
        .map_err(|why| if why.contains("Secrets were required") { "The password was not accepted.".to_string() } else { why })?;
    let _ = run(8, "nmcli", &["connection", "modify", &id, "connection.autoconnect", "yes"]);
    Ok(())
}

/// The hotspot's network name and password.
pub fn hotspot_details() -> Option<(String, String)> {
    let (id, _) = hotspot()?;
    let out = run(4, "nmcli", &["-s", "-g", "802-11-wireless.ssid,802-11-wireless-security.psk", "connection", "show", &id]).ok()?;
    let mut lines = out.lines();
    Some((lines.next()?.to_string(), lines.next().unwrap_or("").to_string()))
}

/// Why a hotspot name or password cannot be used, if it cannot.
pub fn hotspot_problem(name: &str, password: &str) -> Option<&'static str> {
    if name.trim().is_empty() || name.len() > 32 {
        Some("The name needs 1 to 32 characters.")
    } else if password.chars().count() < 8 || password.len() > 63 {
        Some("The password needs 8 to 63 characters.")
    } else if !line_safe(name) || !line_safe(password) {
        Some("That name or password can't be used here.")
    } else {
        None
    }
}

/// Give the hotspot a new name and password; a running hotspot restarts to use them.
pub fn set_hotspot(name: &str, password: &str) -> Result<(), String> {
    if let Some(problem) = hotspot_problem(name, password) {
        return Err(problem.into());
    }
    let (id, on) = hotspot().ok_or("No hotspot is set up yet.")?;
    // "yes" answers the editor when it asks before saving a profile that connects by itself
    edit(&[&id], &format!("set 802-11-wireless.ssid {name}\nset 802-11-wireless-security.psk {password}\nsave\nyes\nquit\n"))?;
    if on {
        run(45, "nmcli", &["connection", "up", &id])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colons_inside_a_field_are_kept() {
        assert_eq!(fields(r"*:Cafe\: upstairs:72:WPA2"), ["*", "Cafe: upstairs", "72", "WPA2"]);
        assert_eq!(fields(" :Open::"), [" ", "Open", "", ""]);
    }

    #[test]
    fn networks_are_merged_sorted_and_hidden_ones_dropped() {
        let list = " :Home:40:WPA2\n :Home:81:WPA2\n::90:WPA2\n*:Office:55:WPA2 WPA3\n :Library:60:\n";
        let found = parse_networks(list);
        let names: Vec<_> = found.iter().map(|network| network.ssid.as_str()).collect();
        assert_eq!(names, ["Office", "Home", "Library"]);
        assert_eq!(found[1].signal, 81);
        assert!(found[0].active);
        assert!(found[2].open());
        assert!(!found[1].open());
    }

    #[test]
    fn enterprise_and_wep_networks_go_to_the_full_settings() {
        let network = |security: &str| Network { ssid: "x".into(), signal: 50, security: security.into(), active: false, saved: None };
        assert!(network("WPA2").simple());
        assert!(!network("WPA2 802.1X").simple());
        assert!(!network("WEP").simple());
    }

    #[test]
    fn hotspot_names_and_passwords_are_checked() {
        assert!(hotspot_problem("Home", "12345678").is_none());
        assert!(hotspot_problem("", "12345678").is_some());
        assert!(hotspot_problem("Home", "short").is_some());
        assert!(hotspot_problem(&"x".repeat(33), "12345678").is_some());
        assert!(hotspot_problem("Home", "pass\nword1").is_some());
    }

    #[test]
    fn nmcli_errors_lose_their_prefix() {
        assert_eq!(error_text("Error: Connection activation failed: Secrets were required.\n"), "Connection activation failed: Secrets were required.");
        assert_eq!(error_text(""), "NetworkManager did not answer in time.");
    }
}
