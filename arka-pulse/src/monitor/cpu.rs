//! CPU telemetry from `/proc` — load averages and utilisation.
//!
//! Utilisation cannot be read from a single snapshot; it is the change in busy
//! jiffies between two reads of `/proc/stat`. `CpuMeter` holds the previous
//! read so the main loop can ask for utilisation each tick.

use std::fs;
use std::io::{self, ErrorKind};

/// Reads the 1/5/15-minute load averages from `/proc/loadavg`.
pub fn read_loadavg() -> io::Result<(f64, f64, f64)> {
    let s = fs::read_to_string("/proc/loadavg")?;
    let mut it = s.split_whitespace();
    let mut next = || -> io::Result<f64> {
        it.next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "malformed /proc/loadavg"))
    };
    Ok((next()?, next()?, next()?))
}

/// Tracks the previous `/proc/stat` aggregate line to compute utilisation.
pub struct CpuMeter {
    prev_idle: u64,
    prev_total: u64,
    primed: bool,
}

impl CpuMeter {
    pub fn new() -> Self {
        CpuMeter {
            prev_idle: 0,
            prev_total: 0,
            primed: false,
        }
    }

    /// Returns busy percentage since the previous call, or `None` on the first
    /// call (no baseline yet) or when no time has elapsed.
    pub fn sample(&mut self) -> io::Result<Option<f64>> {
        let stat = fs::read_to_string("/proc/stat")?;
        let line = stat
            .lines()
            .next()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "empty /proc/stat"))?;

        // "cpu  user nice system idle iowait irq softirq steal guest guest_nice"
        let vals: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|x| x.parse().ok())
            .collect();
        if vals.len() < 4 {
            return Ok(None);
        }

        let idle = vals[3] + vals.get(4).copied().unwrap_or(0); // idle + iowait
        let total: u64 = vals.iter().sum();

        let util = if self.primed {
            let dt = total.saturating_sub(self.prev_total);
            let di = idle.saturating_sub(self.prev_idle);
            if dt == 0 {
                None
            } else {
                Some((1.0 - (di as f64 / dt as f64)) * 100.0)
            }
        } else {
            None
        };

        self.prev_idle = idle;
        self.prev_total = total;
        self.primed = true;
        Ok(util)
    }
}
