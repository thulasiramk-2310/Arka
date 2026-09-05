//! Shared types for the reliability engine.
//!
//! A `Finding` is the only thing DETECT produces, and it is pure data: a
//! severity, a human summary, and the concrete evidence (real numbers) that
//! justified it. Nothing here can act on the system — that separation is the
//! whole point of the "the model is untrusted" design in
//! `docs/RELIABILITY-ARKA-PULSE.md`.

use std::fmt;

/// How urgent a finding is. Ordered so `>=` comparisons work for "at least this
/// severe" checks and for picking the worst finding in a set.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Ok = 0,
    /// Part of the severity ladder for non-alarming notes; not emitted by the
    /// current rule set, but kept so the scale is complete for later stages.
    #[allow(dead_code)]
    Info = 1,
    Warning = 2,
    Critical = 3,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Ok => "OK",
            Severity::Info => "INFO",
            Severity::Warning => "WARN",
            Severity::Critical => "CRIT",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One observation from the deterministic detector.
///
/// `domain` names the subsystem (`cpu`, `memory`, `thermal`, ...), `summary`
/// is plain language, and `evidence` carries the measured values so a reader
/// can see exactly why the finding fired. `evidence` is fact, never inference.
pub struct Finding {
    pub domain: &'static str,
    pub severity: Severity,
    pub summary: String,
    pub evidence: String,
}

impl Finding {
    pub fn new(
        domain: &'static str,
        severity: Severity,
        summary: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Finding {
            domain,
            severity,
            summary: summary.into(),
            evidence: evidence.into(),
        }
    }
}
