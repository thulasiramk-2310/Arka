#[derive(Debug, Clone)]
pub struct Network {
    pub ssid:    String,
    pub signal:  u8,
    pub secured: bool,
    pub in_use:  bool,
}

pub fn scan() -> Vec<Network> {
    let out = std::process::Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"])
        .output()
        .ok();
    let Some(out) = out else { return Vec::new() };
    let text = String::from_utf8_lossy(&out.stdout);

    let mut nets: Vec<Network> = text.lines().filter_map(parse_line).collect();
    nets.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    nets.dedup_by(|a, b| a.ssid == b.ssid);
    nets
}

fn parse_line(line: &str) -> Option<Network> {
    let parts: Vec<&str> = line.splitn(4, ':').collect();
    if parts.len() < 3 { return None; }
    let in_use  = parts[0] == "*";
    let ssid    = parts[1].trim().to_string();
    if ssid.is_empty() || ssid == "--" { return None; }
    let signal: u8 = parts[2].parse().unwrap_or(0);
    let secured = parts.get(3).map(|s| !s.trim().is_empty() && s.trim() != "--").unwrap_or(false);
    Some(Network { ssid, signal, secured, in_use })
}

pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let mut cmd = std::process::Command::new("nmcli");
    cmd.args(["device", "wifi", "connect", ssid]);
    if let Some(pw) = password {
        cmd.args(["password", pw]);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
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
