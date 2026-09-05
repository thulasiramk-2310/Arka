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
use std::time::{SystemTime, UNIX_EPOCH};

use crate::detect;
use crate::model::{Finding, Severity};
use crate::monitor::{self, CpuMeter, Telemetry};
use crate::predict::{self, History, Prediction, Sample, HISTORY_CAP};

/// A UI-facing verdict: the single worst *current* severity, the findings that
/// justify it, the forward-looking predictions, and the raw telemetry behind
/// them. `worst` reflects the present (DETECT) only — predictions are separate,
/// carrying their own probability and lead time rather than a severity.
pub struct HealthSnapshot {
    pub worst: Severity,
    pub findings: Vec<Finding>,
    pub predictions: Vec<Prediction>,
    pub telemetry: Telemetry,
}

/// The reliability service ArkaOS components depend on. Read-only by contract.
pub trait ReliabilityService {
    /// Take one health reading. Deterministic given the same system state.
    fn health(&mut self) -> io::Result<HealthSnapshot>;
}

/// Deterministic reliability engine: sample telemetry, run the rule set, and
/// project recent history forward. Holds the history ring buffer across calls.
pub struct PulseEngine {
    meter: CpuMeter,
    ncpu: f64,
    history: History,
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
        PulseEngine {
            meter,
            ncpu,
            history: History::new(HISTORY_CAP),
        }
    }

    fn now_secs() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
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

        // Record this reading so PREDICT has a trend to fit.
        let swap_pct = if telemetry.memory.swap_total_kb > 0 {
            Some(telemetry.memory.swap_used_pct())
        } else {
            None
        };
        self.history.push(Sample {
            t: Self::now_secs(),
            mem_pct: telemetry.memory.used_pct(),
            swap_pct,
            temp_max: telemetry.thermal.max_c(),
        });

        let findings: Vec<Finding> = detect::evaluate(&telemetry, self.ncpu);
        let predictions: Vec<Prediction> = predict::evaluate(&self.history);
        let worst = findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(Severity::Ok);
        Ok(HealthSnapshot {
            worst,
            findings,
            predictions,
            telemetry,
        })
    }
}
