//! EXPLAIN — turn evidence into a plain-language account, safely.
//!
//! The boundary this module enforces, in code:
//!
//! ```text
//! Telemetry → DETECT/PREDICT evidence → sanitise → [model] → validate → Explanation
//! ```
//!
//! An explainer answers *"what is happening, what evidence supports it, and
//! what action might help?"* — it never answers *"run this command."* The only
//! thing that can ever become an action is an [`Intent`] drawn from a fixed set;
//! a model that emits anything else is rejected by [`validate`], not executed.
//!
//! Today the only explainer is [`FallbackExplainer`] — deterministic, no model.
//! A future local-LLM backend implements the same [`Explainer`] trait, and its
//! output is forced through [`validate`]; on any failure it degrades to the
//! fallback. No model output ever reaches the rest of the system unvalidated.

pub mod sanitize;

use crate::model::{Finding, Severity};
use crate::predict::Prediction;

/// Risk tier of a proposed action — the input to the (future) policy engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
            Risk::Critical => "critical",
        }
    }
}

impl std::fmt::Display for Risk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The *fixed* set of intents an explanation may propose. This is the action
/// registry's vocabulary: a model can only ever name one of these, and even
/// then RECOVER (not built) would still gate it through the policy engine.
/// Nothing here is executed by this crate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    None,
    InvestigateManually,
    FreeReclaimableMemory,
    RestartNetworkService,
    ReduceThermalLoad,
}

impl Intent {
    pub fn id(self) -> &'static str {
        match self {
            Intent::None => "NONE",
            Intent::InvestigateManually => "INVESTIGATE_MANUALLY",
            Intent::FreeReclaimableMemory => "FREE_RECLAIMABLE_MEMORY",
            Intent::RestartNetworkService => "RESTART_NETWORK_SERVICE",
            Intent::ReduceThermalLoad => "REDUCE_THERMAL_LOAD",
        }
    }

    /// Conservative risk tiers. Anything that touches a running service is at
    /// least Medium, so it can never be auto-applied without approval later.
    pub fn risk(self) -> Risk {
        match self {
            Intent::None | Intent::InvestigateManually => Risk::Low,
            Intent::FreeReclaimableMemory => Risk::Low,
            Intent::RestartNetworkService => Risk::Medium,
            Intent::ReduceThermalLoad => Risk::Medium,
        }
    }

    /// Map a raw string (e.g. from model JSON) to a known intent, or fail.
    pub fn parse(s: &str) -> Result<Intent, ValidationError> {
        match s.trim().to_ascii_uppercase().as_str() {
            "NONE" => Ok(Intent::None),
            "INVESTIGATE_MANUALLY" => Ok(Intent::InvestigateManually),
            "FREE_RECLAIMABLE_MEMORY" => Ok(Intent::FreeReclaimableMemory),
            "RESTART_NETWORK_SERVICE" => Ok(Intent::RestartNetworkService),
            "REDUCE_THERMAL_LOAD" => Ok(Intent::ReduceThermalLoad),
            other => Err(ValidationError::UnknownIntent(other.to_string())),
        }
    }
}

/// Where an explanation came from — surfaced so the UI can be honest about it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// Deterministic, built from the metrics with no model involved.
    Fallback,
    /// Produced by a model and passed through [`validate`].
    Model,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Fallback => "fallback",
            Source::Model => "model",
        }
    }
}

/// A validated, safe-to-show account of an incident.
pub struct Explanation {
    pub diagnosis: String,
    pub impact: String,
    pub evidence: Vec<String>,
    pub intent: Intent,
    pub source: Source,
}

/// Raw fields as they would arrive from a model's JSON, before validation.
pub struct RawExplanation {
    pub diagnosis: String,
    pub impact: String,
    pub evidence: Vec<String>,
    pub intent: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// Missing/empty/over-long fields — not schema-valid.
    Schema(&'static str),
    /// A field contained a command-like/dangerous string.
    DangerousString(&'static str),
    /// The proposed intent is not in the action registry.
    UnknownIntent(String),
}

const MAX_TEXT: usize = 500;
const MAX_EVIDENCE_ITEMS: usize = 20;
const MAX_EVIDENCE_LEN: usize = 300;

/// Substrings that must never appear in explanation text — a model emitting
/// these is trying to describe or smuggle a command, so the whole output is
/// rejected (defence in depth; the real guarantee is the fixed [`Intent`] set).
const DANGEROUS: &[&str] = &[
    "rm -rf", "mkfs", "dd if=", ":(){", "$(", "`", "sudo ", "chmod 777",
    "/dev/tcp", "bash -c", "sh -c", "eval ", "curl ", "wget ", "| sh", "|sh",
    "> /", ">/etc", "nc -",
];

fn contains_dangerous(s: &str) -> bool {
    let low = s.to_ascii_lowercase();
    DANGEROUS.iter().any(|p| low.contains(&p.to_ascii_lowercase()))
}

/// The "model is untrusted" gate. Every path from model output to something the
/// system shows or acts on passes through here.
pub fn validate(raw: &RawExplanation) -> Result<Explanation, ValidationError> {
    if raw.diagnosis.trim().is_empty() {
        return Err(ValidationError::Schema("diagnosis empty"));
    }
    if raw.impact.trim().is_empty() {
        return Err(ValidationError::Schema("impact empty"));
    }
    if raw.diagnosis.len() > MAX_TEXT || raw.impact.len() > MAX_TEXT {
        return Err(ValidationError::Schema("field too long"));
    }
    if raw.evidence.len() > MAX_EVIDENCE_ITEMS {
        return Err(ValidationError::Schema("too many evidence items"));
    }
    if raw.evidence.iter().any(|e| e.len() > MAX_EVIDENCE_LEN) {
        return Err(ValidationError::Schema("evidence item too long"));
    }

    if contains_dangerous(&raw.diagnosis) {
        return Err(ValidationError::DangerousString("diagnosis"));
    }
    if contains_dangerous(&raw.impact) {
        return Err(ValidationError::DangerousString("impact"));
    }
    if raw.evidence.iter().any(|e| contains_dangerous(e)) {
        return Err(ValidationError::DangerousString("evidence"));
    }

    let intent = Intent::parse(&raw.intent)?;

    Ok(Explanation {
        diagnosis: sanitize::sanitize(&raw.diagnosis),
        impact: sanitize::sanitize(&raw.impact),
        evidence: raw.evidence.iter().map(|e| sanitize::sanitize(e)).collect(),
        intent,
        source: Source::Model,
    })
}

/// The bundle of evidence handed to an explainer.
pub struct Incident<'a> {
    pub findings: &'a [Finding],
    pub predictions: &'a [Prediction],
}

/// Anything that can produce an [`Explanation`] from an [`Incident`].
///
/// A future `LlmExplainer` implements this by building a sanitised prompt,
/// running local inference, parsing the JSON, and calling [`validate`] — then
/// returning the fallback if any of that fails.
pub trait Explainer {
    fn explain(&self, incident: &Incident) -> Explanation;
}

/// Deterministic explainer — no model, never fails, always available.
pub struct FallbackExplainer;

impl Explainer for FallbackExplainer {
    fn explain(&self, inc: &Incident) -> Explanation {
        // A present problem outranks a forecast.
        let worst = inc
            .findings
            .iter()
            .filter(|f| f.severity >= Severity::Warning)
            .max_by_key(|f| f.severity);

        let (diagnosis, impact, evidence, intent) = if let Some(f) = worst {
            (
                f.summary.clone(),
                impact_for(f.domain).to_string(),
                vec![f.evidence.clone()],
                intent_for(f.domain),
            )
        } else if let Some(p) = inc.predictions.first() {
            (
                p.summary.clone(),
                format!("If the trend continues: {}", impact_for(p.domain)),
                vec![format!("{} — estimated, heuristic", p.evidence)],
                // Forecasts surface for attention; they never propose an action.
                Intent::InvestigateManually,
            )
        } else {
            (
                "System healthy".to_string(),
                "None — no action needed.".to_string(),
                Vec::new(),
                Intent::None,
            )
        };

        Explanation {
            diagnosis: sanitize::sanitize(&diagnosis),
            impact: sanitize::sanitize(&impact),
            evidence: evidence.iter().map(|e| sanitize::sanitize(e)).collect(),
            intent,
            source: Source::Fallback,
        }
    }
}

fn impact_for(domain: &str) -> &'static str {
    match domain {
        "memory" => "Applications may be terminated to reclaim memory (OOM).",
        "thermal" => "The CPU may throttle, or the machine may shut down to protect itself.",
        "cpu" => "The system may become sluggish and slow to respond.",
        "io" => "Disk operations may stall, making the system feel frozen.",
        _ => "System behaviour may degrade.",
    }
}

fn intent_for(domain: &str) -> Intent {
    match domain {
        "memory" => Intent::FreeReclaimableMemory,
        "thermal" => Intent::ReduceThermalLoad,
        _ => Intent::InvestigateManually,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(diag: &str, impact: &str, intent: &str) -> RawExplanation {
        RawExplanation {
            diagnosis: diag.to_string(),
            impact: impact.to_string(),
            evidence: vec!["mem 96% used".to_string()],
            intent: intent.to_string(),
        }
    }

    #[test]
    fn valid_model_output_passes() {
        let r = raw("Memory is nearly full", "Apps may be killed", "FREE_RECLAIMABLE_MEMORY");
        let e = validate(&r).expect("well-formed output should validate");
        assert_eq!(e.intent, Intent::FreeReclaimableMemory);
        assert_eq!(e.source, Source::Model);
    }

    #[test]
    fn unknown_intent_is_rejected() {
        let r = raw("x", "y", "DELETE_ALL_LOGS");
        assert!(matches!(validate(&r), Err(ValidationError::UnknownIntent(_))));
    }

    #[test]
    fn command_like_output_is_rejected() {
        let r = raw("run `rm -rf /var` to fix", "bad", "NONE");
        assert!(matches!(validate(&r), Err(ValidationError::DangerousString(_))));
    }

    #[test]
    fn empty_field_is_rejected() {
        let r = raw("", "impact", "NONE");
        assert!(matches!(validate(&r), Err(ValidationError::Schema(_))));
    }

    #[test]
    fn service_touching_intent_needs_at_least_medium_risk() {
        assert_eq!(Intent::RestartNetworkService.risk(), Risk::Medium);
        assert_eq!(Intent::None.risk(), Risk::Low);
    }

    #[test]
    fn fallback_explains_worst_finding() {
        let findings = vec![
            Finding::new("cpu", Severity::Warning, "busy", "load high"),
            Finding::new("memory", Severity::Critical, "almost full", "97% used"),
        ];
        let inc = Incident { findings: &findings, predictions: &[] };
        let e = FallbackExplainer.explain(&inc);
        assert_eq!(e.source, Source::Fallback);
        assert_eq!(e.intent, Intent::FreeReclaimableMemory); // from the memory finding
        assert!(e.impact.contains("OOM"));
    }
}
