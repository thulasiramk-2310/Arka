//! MONITOR — read-only telemetry collection.
//!
//! Every reader in this module only ever *reads* `/proc` and `/sys`. Nothing
//! here opens a writable handle, spawns a process, or touches the network.
//! That is a hard invariant of the reliability engine: observation must never
//! be able to change the thing it observes.

pub mod cpu;
pub mod memory;
pub mod psi;
pub mod thermal;

pub use cpu::CpuMeter;
use memory::Memory;
use psi::Psi;
use thermal::Thermal;

use std::io;

/// One complete snapshot of system health signals at a moment in time.
pub struct Telemetry {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    /// CPU busy % since the previous sample; `None` on the first sample.
    pub cpu_util: Option<f64>,
    pub memory: Memory,
    pub psi: Psi,
    pub thermal: Thermal,
}

/// Collect one telemetry snapshot. `meter` carries CPU state between samples.
pub fn sample(meter: &mut CpuMeter) -> io::Result<Telemetry> {
    let (load1, load5, load15) = cpu::read_loadavg()?;
    let cpu_util = meter.sample()?;
    let memory = memory::read()?;
    let psi = psi::read();
    let thermal = thermal::read();
    Ok(Telemetry {
        load1,
        load5,
        load15,
        cpu_util,
        memory,
        psi,
        thermal,
    })
}
