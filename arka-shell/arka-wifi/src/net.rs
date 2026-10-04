#[derive(Debug, Clone)]
pub struct Network {
    pub ssid:    String,
    pub signal:  u8,
    pub secured: bool,
    pub in_use:  bool,
}

pub fn scan() -> Result<Vec<Network>, String> {
    let out = std::process::Command::new("timeout")
        .args(["--kill-after=2", "15", "nmcli", "-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() { return Err("WiFi scan failed or timed out".into()); }
    let text = String::from_utf8_lossy(&out.stdout);

    let mut nets: Vec<Network> = text.lines().filter_map(parse_line).collect();
    nets.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    let mut seen = std::collections::HashSet::new();
    nets.retain(|n| seen.insert(n.ssid.clone()));
    Ok(nets)
}

fn parse_line(line: &str) -> Option<Network> {
    let mut parts = vec![String::new()];
    let mut escaped = false;
    for c in line.chars() {
        if escaped { parts.last_mut()?.push(c); escaped = false; }
        else if c == '\\' { escaped = true; }
        else if c == ':' { parts.push(String::new()); }
        else { parts.last_mut()?.push(c); }
    }
    if escaped || parts.len() != 4 { return None; }
    if parts.len() < 3 { return None; }
    let in_use  = parts[0] == "*";
    let ssid    = parts[1].clone();
    if ssid.is_empty() || ssid == "--" { return None; }
    let signal: u8 = parts[2].parse().unwrap_or(0);
    let secured = parts.get(3).map(|s| !s.trim().is_empty() && s.trim() != "--").unwrap_or(false);
    Some(Network { ssid, signal, secured, in_use })
}

pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;
    if password.is_some_and(|pw| pw.contains(['\n', '\r'])) {
        return Err("The network password cannot contain a line break".into());
    }
    let mut cmd = std::process::Command::new("timeout");
    cmd.args(["--kill-after=2", "45", "nmcli", "--wait", "30", "--ask", "device", "wifi", "connect", ssid])
        .env("LC_ALL", "C").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(mut input) = child.stdin.take() {
        if let Some(pw) = password {
            if writeln!(input, "{pw}").is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Could not send credentials to the network service".into());
            }
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else {
        // Do not echo interactive output, which may include a credential prompt.
        Err("Connection failed or timed out. Check the password and network availability.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_escaped_ssids_without_changing_the_name() {
        let n = parse_line(r"*:Office\: Guest\\WiFi:87:WPA2").unwrap();
        assert_eq!(n.ssid, r"Office: Guest\WiFi");
        assert_eq!(n.signal, 87);
        assert!(n.in_use && n.secured);
        assert_eq!(parse_line(": space :12:--").unwrap().ssid, " space ");
        assert!(parse_line(":bad:field:count:extra").is_none());
    }
}

pub fn signal_bars(signal: u8) -> &'static str {
    match signal {
        75..=100 => "▂▄▆█",
        50..=74  => "▂▄▆░",
        25..=49  => "▂▄░░",
        _        => "▂░░░",
    }
}
