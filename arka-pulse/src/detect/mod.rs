//! DETECT — deterministic rules over a telemetry snapshot.
//!
//! This is a plain threshold rule engine: given one `Telemetry` reading and the
//! CPU count, it returns the `Finding`s that fired. It is intentionally simple
//! and fully deterministic — the same input always yields the same output, and
//! every finding carries the real numbers that triggered it.
//!
//! The multivariate anomaly detection (an isolation-forest baseline) and the
//! temporal *prediction* stage described in `docs/RELIABILITY-ARKA-PULSE.md`
//! are deliberately **not** here yet. This is the honest floor: rules only.

use crate::model::{Finding, Severity};
use crate::monitor::Telemetry;

/// Thresholds, named so the policy is legible and tunable in one place.
mod thresh {
    pub const MEM_WARN_PCT: f64 = 90.0;
    pub const MEM_CRIT_PCT: f64 = 97.0;
    pub const SWAP_WARN_PCT: f64 = 80.0;
    /// Load average per CPU at which the machine is meaningfully oversubscribed.
    pub const LOAD_PER_CPU_WARN: f64 = 2.0;
    /// PSI "some avg10" (% of the last 10s stalled) considered notable.
    pub const PSI_WARN: f64 = 40.0;
    pub const TEMP_WARN_C: f64 = 85.0;
    pub const TEMP_CRIT_C: f64 = 95.0;
}

/// Evaluate one snapshot. `ncpu` is the logical CPU count (>= 1.0).
pub fn evaluate(t: &Telemetry, ncpu: f64) -> Vec<Finding> {
    let mut out = Vec::new();

    // --- Memory --------------------------------------------------------------
    let mem = t.memory.used_pct();
    if mem >= thresh::MEM_CRIT_PCT {
        out.push(Finding::new(
            "memory",
            Severity::Critical,
            "Memory almost exhausted — the OOM killer is imminent",
            format!("{mem:.0}% used (>= {:.0}%)", thresh::MEM_CRIT_PCT),
        ));
    } else if mem >= thresh::MEM_WARN_PCT {
        out.push(Finding::new(
            "memory",
            Severity::Warning,
            "Memory usage is high",
            format!("{mem:.0}% used (>= {:.0}%)", thresh::MEM_WARN_PCT),
        ));
    }

    if t.memory.swap_total_kb > 0 {
        let swap = t.memory.swap_used_pct();
        if swap >= thresh::SWAP_WARN_PCT {
            out.push(Finding::new(
                "memory",
                Severity::Warning,
                "Swap is filling up — expect stalls",
                format!("swap {swap:.0}% used (>= {:.0}%)", thresh::SWAP_WARN_PCT),
            ));
        }
    }

    // --- CPU -----------------------------------------------------------------
    if ncpu > 0.0 {
        let per = t.load1 / ncpu;
        if per >= thresh::LOAD_PER_CPU_WARN {
            out.push(Finding::new(
                "cpu",
                Severity::Warning,
                "Sustained CPU oversubscription",
                format!(
                    "load1 {:.2} over {:.0} CPUs = {per:.2}/cpu (>= {:.1})",
                    t.load1, ncpu, thresh::LOAD_PER_CPU_WARN
                ),
            ));
        }
    }
    if let Some(p) = t.psi.cpu_some10 {
        if p >= thresh::PSI_WARN {
            out.push(Finding::new(
                "cpu",
                Severity::Warning,
                "Tasks are stalling on CPU",
                format!("PSI cpu some avg10 {p:.1}% (>= {:.0}%)", thresh::PSI_WARN),
            ));
        }
    }
    if let Some(p) = t.psi.mem_some10 {
        if p >= thresh::PSI_WARN {
            out.push(Finding::new(
                "memory",
                Severity::Warning,
                "Tasks are stalling on memory",
                format!("PSI memory some avg10 {p:.1}% (>= {:.0}%)", thresh::PSI_WARN),
            ));
        }
    }

    // --- I/O -----------------------------------------------------------------
    if let Some(p) = t.psi.io_some10 {
        if p >= thresh::PSI_WARN {
            out.push(Finding::new(
                "io",
                Severity::Warning,
                "Storage I/O is stalling tasks",
                format!("PSI io some avg10 {p:.1}% (>= {:.0}%)", thresh::PSI_WARN),
            ));
        }
    }

    // --- Thermal -------------------------------------------------------------
    if let Some(c) = t.thermal.max_c() {
        if c >= thresh::TEMP_CRIT_C {
            out.push(Finding::new(
                "thermal",
                Severity::Critical,
                "Critical temperature — thermal throttling or shutdown risk",
                format!("{c:.0}°C (>= {:.0}°C)", thresh::TEMP_CRIT_C),
            ));
        } else if c >= thresh::TEMP_WARN_C {
            out.push(Finding::new(
                "thermal",
                Severity::Warning,
                "Running hot",
                format!("{c:.0}°C (>= {:.0}°C)", thresh::TEMP_WARN_C),
            ));
        }
    }

    // --- Healthy -------------------------------------------------------------
    if out.is_empty() {
        out.push(Finding::new(
            "system",
            Severity::Ok,
            "System healthy — no action needed",
            format!(
                "mem {:.0}% · load {:.2} · {}",
                mem,
                t.load1,
                match t.thermal.max_c() {
                    Some(c) => format!("{c:.0}°C"),
                    None => "temp n/a".to_string(),
                }
            ),
        ));
    }

    out
}
