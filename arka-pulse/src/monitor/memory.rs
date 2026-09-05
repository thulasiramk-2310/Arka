//! Memory and swap telemetry from `/proc/meminfo`.

use std::fs;
use std::io;

#[derive(Default)]
pub struct Memory {
    pub total_kb: u64,
    pub available_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
}

impl Memory {
    /// Percentage of RAM in use (0–100), based on `MemAvailable`.
    pub fn used_pct(&self) -> f64 {
        if self.total_kb == 0 {
            0.0
        } else {
            (1.0 - self.available_kb as f64 / self.total_kb as f64) * 100.0
        }
    }

    /// Percentage of swap in use (0–100); 0 when there is no swap.
    pub fn swap_used_pct(&self) -> f64 {
        if self.swap_total_kb == 0 {
            0.0
        } else {
            (1.0 - self.swap_free_kb as f64 / self.swap_total_kb as f64) * 100.0
        }
    }
}

pub fn read() -> io::Result<Memory> {
    let text = fs::read_to_string("/proc/meminfo")?;
    let mut m = Memory::default();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(key), Some(val)) = (it.next(), it.next()) else {
            continue;
        };
        let val: u64 = match val.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        match key {
            "MemTotal:" => m.total_kb = val,
            "MemAvailable:" => m.available_kb = val,
            "SwapTotal:" => m.swap_total_kb = val,
            "SwapFree:" => m.swap_free_kb = val,
            _ => {}
        }
    }
    Ok(m)
}
