//! Thermal telemetry from `/sys/class/thermal/thermal_zone*`.
//!
//! Each zone exposes `temp` in millidegrees Celsius and a `type` name. Zones
//! come and go and may be unreadable in a VM, so failures are skipped rather
//! than propagated — an empty set of zones is a valid reading.

use std::fs;

#[derive(Default)]
pub struct Thermal {
    /// (zone type, temperature °C) for every readable zone.
    pub zones: Vec<(String, f64)>,
}

impl Thermal {
    /// The hottest zone's temperature, or `None` if no zone could be read.
    pub fn max_c(&self) -> Option<f64> {
        self.zones
            .iter()
            .map(|(_, c)| *c)
            .fold(None, |m, c| Some(m.map_or(c, |mx: f64| mx.max(c))))
    }
}

pub fn read() -> Thermal {
    let mut zones = Vec::new();
    let dir = match fs::read_dir("/sys/class/thermal") {
        Ok(d) => d,
        Err(_) => return Thermal::default(),
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let base = entry.path();
        let milli = match fs::read_to_string(base.join("temp")) {
            Ok(s) => match s.trim().parse::<i64>() {
                Ok(v) => v,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let kind = fs::read_to_string(base.join("type"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| name.into_owned());
        zones.push((kind, milli as f64 / 1000.0));
    }
    Thermal { zones }
}
