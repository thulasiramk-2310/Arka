//! Tool registry. Adding a tool = one row in `REGISTRY` + a runner arm (+ arg
//! validation for writes). Read tools run automatically; write tools are
//! approval-gated by the agent loop before `run_write` is ever called (rule 3).
//!
//! Read tools return both a human string (`output`) and typed `Fact`s: the
//! agent collects the facts and verifies the model's final answer against them
//! (see `facts`), so the model can never state an unbacked number or status.

use serde_json::Value;

use crate::backend::SystemBackend;
use crate::config::Config;
use crate::facts::Fact;

pub mod arkad;
pub mod pulse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Write,
}

pub struct ToolSpec {
    pub name: &'static str,
    pub kind: ToolKind,
    pub description: &'static str,
    pub args_hint: &'static str,
}

pub const REGISTRY: &[ToolSpec] = &[
    ToolSpec {
        name: "system_status",
        kind: ToolKind::Read,
        description:
            "Current privacy state from arkad: score + MAC/DNS/hostname/IPv6/sandbox/browser.",
        args_hint: "{}",
    },
    ToolSpec {
        name: "pulse_health",
        kind: ToolKind::Read,
        description:
            "System health from arka-pulse: worst severity, findings, key telemetry (CPU/mem/temp).",
        args_hint: "{}",
    },
    ToolSpec {
        name: "enforce_privacy",
        kind: ToolKind::Write,
        description: "Re-apply every privacy enforcer via arkad (EnforceAll).",
        args_hint: "{}",
    },
    ToolSpec {
        name: "restart_service",
        kind: ToolKind::Write,
        description: "Restart an allow-listed systemd unit.",
        args_hint: "{\"unit\":\"NetworkManager\"}",
    },
    // NOTE: there is deliberately no `set_privacy_setting` tool. arkad has no
    // per-setting setter, so a tool that always fails would only waste the
    // model's steps. It returns once arkad grows a real SetSetting method.
];

/// TEST-ONLY tool that stands in for a future protection-lowering setter, so the
/// typed-confirmation path (#4) can be proven end-to-end. It cannot exist in a
/// release build (there is no real weakening tool yet — arkad has no setter).
#[cfg(test)]
pub static TEST_WEAKEN: ToolSpec = ToolSpec {
    name: "test_lower_protection",
    kind: ToolKind::Write,
    description: "TEST-ONLY: simulate lowering a privacy protection.",
    args_hint: "{}",
};

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    #[cfg(test)]
    if name == TEST_WEAKEN.name {
        return Some(&TEST_WEAKEN);
    }
    REGISTRY.iter().find(|t| t.name == name)
}

/// Does this write tool lower the device's protection? Such writes need a typed
/// confirmation, not just a y/N (#4). No shipping tool does yet — when a real
/// setter lands, list it here so the stronger gate applies automatically.
pub fn weakens_protection(name: &str) -> bool {
    #[cfg(test)]
    if name == "test_lower_protection" {
        return true;
    }
    let _ = name;
    false
}

pub fn names() -> String {
    REGISTRY
        .iter()
        .map(|t| t.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A tool's result: `output` is human/data text (fenced as data by the agent),
/// `facts` are the typed truths the answer verifier is allowed to rely on.
pub struct ToolOutput {
    pub output: String,
    pub facts: Vec<Fact>,
}

impl ToolOutput {
    pub fn text(s: impl Into<String>) -> Self {
        ToolOutput {
            output: s.into(),
            facts: vec![],
        }
    }
    pub fn with_facts(s: impl Into<String>, facts: Vec<Fact>) -> Self {
        ToolOutput {
            output: s.into(),
            facts,
        }
    }
}

/// Validate write-tool args BEFORE approval, so bad args reject without ever
/// prompting or touching the system (rule 4, fail closed).
pub fn validate_write(name: &str, args: &Value, cfg: &Config) -> anyhow::Result<()> {
    match name {
        #[cfg(test)]
        "test_lower_protection" => Ok(()),
        "enforce_privacy" => Ok(()),
        "restart_service" => {
            let unit = args
                .get("unit")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("bad args: expected {{\"unit\": string}}"))?;
            if cfg.allowed_units.iter().any(|u| u == unit) {
                Ok(())
            } else {
                anyhow::bail!(
                    "unit '{unit}' is not in the allow-list {:?}",
                    cfg.allowed_units
                )
            }
        }
        other => anyhow::bail!("'{other}' is not a write tool"),
    }
}

pub async fn run_read<B: SystemBackend>(
    backend: &B,
    name: &str,
    _args: &Value,
) -> anyhow::Result<ToolOutput> {
    match name {
        "system_status" => arkad::system_status(backend).await,
        "pulse_health" => pulse::pulse_health(backend).await,
        other => anyhow::bail!("'{other}' is not a read tool"),
    }
}

/// Run a write tool. Only reached after explicit approval (rule 3).
///
/// When `dry_run` is set, this returns a description of what WOULD happen and
/// never calls a backend write method (proven by a test). Args are assumed
/// already validated by `validate_write`.
pub async fn run_write<B: SystemBackend>(
    backend: &B,
    name: &str,
    args: &Value,
    dry_run: bool,
    _cfg: &Config,
) -> anyhow::Result<ToolOutput> {
    match name {
        #[cfg(test)]
        "test_lower_protection" => Ok(ToolOutput::text(if dry_run {
            "[dry-run] would lower protection — no change made"
        } else {
            "[test] lowered protection"
        })),
        "enforce_privacy" => {
            if dry_run {
                return Ok(ToolOutput::text(
                    "[dry-run] would call arkad EnforceAll() — no change made",
                ));
            }
            Ok(ToolOutput::text(backend.enforce_all().await?))
        }
        "restart_service" => {
            let unit = args.get("unit").and_then(|v| v.as_str()).unwrap_or("");
            if dry_run {
                return Ok(ToolOutput::text(format!(
                    "[dry-run] would restart {unit} — no change made"
                )));
            }
            Ok(ToolOutput::text(backend.restart_service(unit).await?))
        }
        other => anyhow::bail!("'{other}' is not a write tool"),
    }
}
