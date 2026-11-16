//! Bluetooth devices, sound outputs, the battery and the power mode, read and changed through the
//! usual tools (bluetoothctl, pactl, powerprofilesctl). Every call has a time limit: a hardware
//! tool that hangs (bluetoothctl with no adapter does) must not hang the desktop.

use std::process::Command;

fn run(seconds: u32, program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("timeout")
        .arg(seconds.to_string())
        .arg(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(text)
    } else {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if said.is_empty() { text } else { said })
    }
}

/// What the quick settings show besides the switches.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Extras {
    /// Charge in percent and whether it is charging, when there is a battery.
    pub battery: Option<(u8, Charge)>,
    /// "power-saver", "balanced" or "performance", when the power modes are available.
    pub power: Option<String>,
    /// The sound outputs (name, description) and which one is in use.
    pub outputs: Vec<(String, String)>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Charge {
    Charging,
    Full,
    OnBattery,
}

pub fn read_extras() -> Extras {
    Extras {
        battery: battery(),
        power: run(3, "powerprofilesctl", &["get"]).ok().filter(|mode| !mode.is_empty()),
        outputs: run(3, "pactl", &["list", "sinks"]).map(|text| parse_sinks(&text)).unwrap_or_default(),
        output: run(3, "pactl", &["get-default-sink"]).ok().filter(|name| !name.is_empty()),
    }
}

fn battery() -> Option<(u8, Charge)> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?.flatten().map(|entry| entry.path()).find(|path| {
        std::fs::read_to_string(path.join("type")).is_ok_and(|kind| kind.trim() == "Battery")
            && path.join("capacity").exists()
    })?;
    let level = std::fs::read_to_string(dir.join("capacity")).ok()?.trim().parse().ok()?;
    let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
    Some((level, parse_charge(&status)))
}

pub fn parse_charge(status: &str) -> Charge {
    match status.trim() {
        "Charging" => Charge::Charging,
        "Full" | "Not charging" => Charge::Full,
        _ => Charge::OnBattery,
    }
}

/// The outputs in `pactl list sinks` (C locale): each `Name:` with the `Description:` after it.
pub fn parse_sinks(text: &str) -> Vec<(String, String)> {
    let mut sinks = Vec::new();
    let mut name: Option<String> = None;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("Name: ") {
            name = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("Description: ") {
            if let Some(name) = name.take() {
                sinks.push((name, rest.to_string()));
            }
        }
    }
    sinks
}

pub fn set_power(mode: &str) -> Result<(), String> {
    run(5, "powerprofilesctl", &["set", mode]).map(|_| ())
}

pub fn set_output(name: &str) -> Result<(), String> {
    run(5, "pactl", &["set-default-sink", name]).map(|_| ())
}

/// One Bluetooth device.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

/// `Device AA:BB:CC:DD:EE:FF Name` lines from `bluetoothctl devices`.
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Device ")?;
            let (address, name) = rest.split_once(' ').unwrap_or((rest, ""));
            Some((address.to_string(), name.trim().to_string()))
        })
        .collect()
}

/// A name that is only the address again (or empty) says nothing; such devices are left out of
/// the list of new ones.
fn has_real_name(address: &str, name: &str) -> bool {
    !name.is_empty() && name.replace('-', ":") != address
}

fn powered() -> Result<(), String> {
    let show = run(4, "bluetoothctl", &["show"]).map_err(|_| "No Bluetooth adapter answered.".to_string())?;
    if show.contains("Powered: yes") {
        Ok(())
    } else {
        Err("Bluetooth is off.".into())
    }
}

/// The paired devices, and when `look` is set, new ones found by looking for 8 seconds.
pub fn devices(look: bool) -> Result<Vec<Device>, String> {
    powered()?;
    if look {
        let _ = run(12, "bluetoothctl", &["--timeout", "8", "scan", "on"]);
    }
    let paired = parse_devices(&run(4, "bluetoothctl", &["devices", "Paired"]).unwrap_or_default());
    let connected = parse_devices(&run(4, "bluetoothctl", &["devices", "Connected"]).unwrap_or_default());
    let mut list: Vec<Device> = paired
        .iter()
        .map(|(address, name)| Device {
            address: address.clone(),
            name: if name.is_empty() { address.clone() } else { name.clone() },
            paired: true,
            connected: connected.iter().any(|(other, _)| other == address),
        })
        .collect();
    if look {
        for (address, name) in parse_devices(&run(4, "bluetoothctl", &["devices"]).unwrap_or_default()) {
            if !list.iter().any(|device| device.address == address) && has_real_name(&address, &name) {
                list.push(Device { address, name, paired: false, connected: false });
            }
        }
    }
    list.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.paired.cmp(&a.paired)).then(a.name.cmp(&b.name)));
    Ok(list)
}

/// Connect a paired device, disconnect a connected one, or pair, trust and connect a new one.
pub fn toggle_device(device: &Device) -> Result<String, String> {
    let address = device.address.as_str();
    if device.connected {
        run(10, "bluetoothctl", &["disconnect", address]).map(|_| format!("{} disconnected", device.name))
    } else {
        if !device.paired {
            run(30, "bluetoothctl", &["pair", address])
                .map_err(|_| format!("{} did not pair. Put it in pairing mode and try again.", device.name))?;
            let _ = run(5, "bluetoothctl", &["trust", address]);
        }
        run(20, "bluetoothctl", &["connect", address])
            .map(|_| format!("{} connected", device.name))
            .map_err(|_| format!("{} did not connect. Is it on and nearby?", device.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sinks_are_read_with_their_descriptions() {
        let text = "Sink #58\n\tState: RUNNING\n\tName: alsa_output.analog\n\tDescription: Speakers\n\nSink #61\n\tName: bluez_output.headset\n\tDescription: WH-1000XM4\n";
        assert_eq!(
            parse_sinks(text),
            [("alsa_output.analog".to_string(), "Speakers".to_string()), ("bluez_output.headset".to_string(), "WH-1000XM4".to_string())]
        );
    }

    #[test]
    fn device_lines_give_address_and_name() {
        let text = "Device 11:22:33:44:55:66 WH-1000XM4\nDevice AA:BB:CC:DD:EE:FF AA-BB-CC-DD-EE-FF\nsomething else\n";
        let found = parse_devices(text);
        assert_eq!(found[0], ("11:22:33:44:55:66".to_string(), "WH-1000XM4".to_string()));
        assert!(has_real_name(&found[0].0, &found[0].1));
        assert!(!has_real_name(&found[1].0, &found[1].1));
    }

    #[test]
    fn battery_states() {
        assert_eq!(parse_charge("Charging\n"), Charge::Charging);
        assert_eq!(parse_charge("Not charging"), Charge::Full);
        assert_eq!(parse_charge("Discharging"), Charge::OnBattery);
    }
}
