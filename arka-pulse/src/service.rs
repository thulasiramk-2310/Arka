//! The reliability *service interface* and its first implementation.
//!
//! This is the ArkaOS service-abstraction discipline applied to reliability:
//! UI and other components depend on the [`ReliabilityService`] trait, never on
//! the concrete engine behind it. Today the only implementation is
//! [`PulseEngine`] (deterministic MONITOR + DETECT); tomorrow it could gain a
//! prediction stage, or be replaced wholesale, without any consumer changing —
//! the same LEGO-swap property `WindowService` gives the desktop
//! (see `docs/FUTURE-CONSIDERATIONS.md`).
//!
//! Crucially, the interface only ever *reports*. There is no method here that
//! changes the system. Any future recovery capability must sit behind its own
//! explicit, policy-gated interface — never smuggled into a "read" call.

use std::io;

use crate::detect;
use crate::model::{Finding, Severity};
use crate::monitor::{self, CpuMeter, Telemetry};

/// A UI-facing verdict: the single worst severity, the findings that justify
/// it, and the raw telemetry they were derived from.
pub struct HealthSnapshot {
    pub worst: Severity,
    pub findings: Vec<Finding>,
    pub telemetry: Telemetry,
}

/// The reliability service ArkaOS components depend on. Read-only by contract.
pub trait ReliabilityService {
    /// Take one health reading. Deterministic given the same system state.
    fn health(&mut self) -> io::Result<HealthSnapshot>;
}

/// Deterministic reliability engine: sample telemetry, run the rule set.
pub struct PulseEngine {
    meter: CpuMeter,
    ncpu: f64,
}

impl PulseEngine {
    pub fn new() -> Self {
        let mut meter = CpuMeter::new();
        // Prime the CPU meter so the first `health()` has a real utilisation
        // baseline rather than `None`.
        let _ = meter.sample();
        let ncpu = std::thread::available_parallelism()
            .map(|n| n.get() as f64)
            .unwrap_or(1.0);
        PulseEngine { meter, ncpu }
    }
}

impl Default for PulseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ReliabilityService for PulseEngine {
    fn health(&mut self) -> io::Result<HealthSnapshot> {
        let telemetry = monitor::sample(&mut self.meter)?;
        let findings: Vec<Finding> = detect::evaluate(&telemetry, self.ncpu);
        let worst = findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(Severity::Ok);
        Ok(HealthSnapshot {
            worst,
            findings,
            telemetry,
        })
    }
}
