//! Read-only D-Bus surface for arka-pulse: `org.arka.pulse`.
//!
//! This mirrors arkad's `org.arka.arkad` service so a desktop consumer — the
//! GTK Privacy Dashboard — can read reliability state exactly the way it reads
//! privacy state: a background proxy onto a read-only interface.
//!
//! **Read-only by contract.** The interface exposes the latest health snapshot
//! and nothing else. There is deliberately no method here that can change the
//! system; if policy-gated recovery ever ships it gets its own explicit
//! interface, never this one. That keeps the "the model is untrusted" and
//! "reliability only ever reports" invariants (see
//! `docs/RELIABILITY-ARKA-PULSE.md`) true at the IPC boundary too.
//!
//! **Honesty at the boundary.** `health`/`summary` come from DETECT and are
//! deterministic — safe for a consumer to present as fact. `predictions` are
//! the trend heuristic: each carries its own probability and confidence, and a
//! consumer must render them as *possible* instability, never as a certain
//! failure. The interface separates the two precisely so the UI can too.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::service::HealthSnapshot;

/// A flat, owned projection of a [`HealthSnapshot`] shaped for D-Bus and for a
/// UI to bind directly. Scalars use `-1.0` as the "unknown" sentinel (D-Bus has
/// no natural null and NaN travels badly), matching the `Option::None` telemetry
/// cases.
#[derive(Clone, Default)]
pub struct PulseSnapshot {
    /// DETECT verdict: `OK` / `WARN` / `CRIT`. Deterministic — presentable as fact.
    pub health: String,
    /// One human line: the explanation diagnosis, else the worst finding, else healthy.
    pub summary: String,
    pub cpu_util: f64,
    pub mem_pct: f64,
    pub load1: f64,
    pub temp_max: f64,
    /// (severity, domain, summary) per current finding.
    pub findings: Vec<(String, String, String)>,
    /// (domain, probability, confidence, summary) per prediction — HEURISTIC.
    pub predictions: Vec<(String, f64, f64, String)>,
}

impl PulseSnapshot {
    /// Project a live snapshot into the D-Bus shape.
    pub fn from_health(s: &HealthSnapshot) -> Self {
        let summary = s
            .explanation
            .as_ref()
            .map(|e| e.diagnosis.clone())
            .or_else(|| s.findings.first().map(|f| f.summary.clone()))
            .unwrap_or_else(|| "System healthy — no action needed".to_string());

        PulseSnapshot {
            health: s.worst.label().to_string(),
            summary,
            cpu_util: s.telemetry.cpu_util.unwrap_or(-1.0),
            mem_pct: s.telemetry.memory.used_pct(),
            load1: s.telemetry.load1,
            temp_max: s.telemetry.thermal.max_c().unwrap_or(-1.0),
            findings: s
                .findings
                .iter()
                .map(|f| {
                    (
                        f.severity.label().to_string(),
                        f.domain.to_string(),
                        f.summary.clone(),
                    )
                })
                .collect(),
            predictions: s
                .predictions
                .iter()
                .map(|p| (p.domain.to_string(), p.probability, p.confidence, p.summary.clone()))
                .collect(),
        }
    }
}

/// Shared latest snapshot: the sampling loop writes it, the interface reads it.
pub type SharedPulse = Arc<RwLock<PulseSnapshot>>;

/// The `org.arka.pulse` interface object.
pub struct PulseIface {
    pub state: SharedPulse,
}

#[zbus::interface(name = "org.arka.pulse")]
impl PulseIface {
    #[zbus(property)]
    async fn health(&self) -> String {
        self.state.read().await.health.clone()
    }

    #[zbus(property)]
    async fn summary(&self) -> String {
        self.state.read().await.summary.clone()
    }

    #[zbus(property)]
    async fn cpu_util(&self) -> f64 {
        self.state.read().await.cpu_util
    }

    #[zbus(property)]
    async fn mem_pct(&self) -> f64 {
        self.state.read().await.mem_pct
    }

    #[zbus(property)]
    async fn load1(&self) -> f64 {
        self.state.read().await.load1
    }

    #[zbus(property)]
    async fn temp_max(&self) -> f64 {
        self.state.read().await.temp_max
    }

    /// (severity, domain, summary) — the current DETECT findings.
    #[zbus(property)]
    async fn findings(&self) -> Vec<(String, String, String)> {
        self.state.read().await.findings.clone()
    }

    /// (domain, probability, confidence, summary) — HEURISTIC trend projections;
    /// a consumer must present these as *possible*, not certain.
    #[zbus(property)]
    async fn predictions(&self) -> Vec<(String, f64, f64, String)> {
        self.state.read().await.predictions.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Finding, Severity};
    use crate::monitor::memory::Memory;
    use crate::monitor::psi::Psi;
    use crate::monitor::thermal::Thermal;
    use crate::monitor::Telemetry;

    fn telemetry(cpu: Option<f64>, temp: Option<f64>) -> Telemetry {
        Telemetry {
            load1: 1.5,
            load5: 1.0,
            load15: 0.8,
            cpu_util: cpu,
            memory: Memory {
                total_kb: 16_000_000,
                available_kb: 12_000_000,
                swap_total_kb: 0,
                swap_free_kb: 0,
            },
            psi: Psi {
                cpu_some10: None,
                mem_some10: None,
                io_some10: None,
            },
            thermal: Thermal {
                zones: match temp {
                    Some(t) => vec![("x86_pkg_temp".to_string(), t)],
                    None => vec![],
                },
            },
        }
    }

    #[test]
    fn healthy_projects_to_ok_with_default_summary() {
        let snap = HealthSnapshot {
            worst: Severity::Ok,
            findings: vec![],
            predictions: vec![],
            explanation: None,
            telemetry: telemetry(Some(5.0), Some(41.0)),
        };
        let p = PulseSnapshot::from_health(&snap);
        assert_eq!(p.health, "OK");
        assert_eq!(p.summary, "System healthy — no action needed");
        assert_eq!(p.temp_max, 41.0);
        assert!(p.findings.is_empty());
    }

    #[test]
    fn unknown_scalars_use_minus_one_sentinel() {
        let snap = HealthSnapshot {
            worst: Severity::Ok,
            findings: vec![],
            predictions: vec![],
            explanation: None,
            telemetry: telemetry(None, None),
        };
        let p = PulseSnapshot::from_health(&snap);
        assert_eq!(p.cpu_util, -1.0);
        assert_eq!(p.temp_max, -1.0);
    }

    #[test]
    fn finding_summary_used_when_no_explanation() {
        let snap = HealthSnapshot {
            worst: Severity::Warning,
            findings: vec![Finding::new("cpu", Severity::Warning, "CPU hot", "88°C")],
            predictions: vec![],
            explanation: None,
            telemetry: telemetry(Some(70.0), Some(88.0)),
        };
        let p = PulseSnapshot::from_health(&snap);
        assert_eq!(p.health, "WARN");
        assert_eq!(p.summary, "CPU hot");
        assert_eq!(p.findings.len(), 1);
        assert_eq!(p.findings[0].0, "WARN");
        assert_eq!(p.findings[0].1, "cpu");
    }
}
