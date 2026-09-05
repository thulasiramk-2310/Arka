//! Pressure Stall Information (PSI) from `/proc/pressure/{cpu,memory,io}`.
//!
//! PSI is the clearest early signal that a resource is becoming a bottleneck —
//! it measures time spent stalled waiting for a resource, before that shows up
//! as a hard failure. It may be absent (older kernel, or `CONFIG_PSI` off), so
//! every field is optional and a missing file is simply `None`, never an error.

use std::fs;

#[derive(Default)]
pub struct Psi {
    /// `some avg10` for CPU — share of time at least one task stalled on CPU.
    pub cpu_some10: Option<f64>,
    pub mem_some10: Option<f64>,
    pub io_some10: Option<f64>,
}

/// Parses the `some avg10=<n>` value from a PSI file, if present.
fn some_avg10(path: &str) -> Option<f64> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("some ") {
            for field in rest.split_whitespace() {
                if let Some(v) = field.strip_prefix("avg10=") {
                    return v.parse().ok();
                }
            }
        }
    }
    None
}

pub fn read() -> Psi {
    Psi {
        cpu_some10: some_avg10("/proc/pressure/cpu"),
        mem_some10: some_avg10("/proc/pressure/memory"),
        io_some10: some_avg10("/proc/pressure/io"),
    }
}
