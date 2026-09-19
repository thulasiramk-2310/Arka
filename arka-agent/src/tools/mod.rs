//! Tool registry. Adding a tool = one row in `REGISTRY` + a runner arm (+ arg
//! validation for writes). Read tools run automatically; write tools are
//! approval-gated by the agent loop before `run_write` is ever called (rule 3).

use serde_json::Value;

use crate::backend::SystemBackend;
use crate::config::Config;

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
        description: "Current privacy state from arkad: score + MAC/DNS/hostname/IPv6/sandbox/browser.",
        args_hint: "{}",
    },
    ToolSpec {
        name: "pulse_health",
        kind: ToolKind::Read,
        description: "System health from arka-pulse: worst severity, findings, key telemetry (CPU/mem/temp).",
        args_hint: "{}",
    },
    ToolSpec {
        name: "enforce_privacy",
        kind: ToolKind::Write,
        description: "Re-apply every privacy enforcer via arkad (EnforceAll).",
        args_hint: "{}",
    },
    ToolSpec {
        name: "set_privacy_setting",
        kind: ToolKind::Write,
        description: "Change one privacy setting. arkad has no setter yet, so this reports 'not implemented in arkad'.",
        args_hint: "{\"setting\":\"mac|dns|hostname|ipv6\",\"enabled\":true|false}",
    },
    ToolSpec {
        name: "restart_service",
        kind: ToolKind::Write,
        description: "Restart an allow-listed systemd unit.",
        args_hint: "{\"unit\":\"NetworkManager\"}",
    },
];

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    REGISTRY.iter().find(|t| t.name == name)
}

pub fn names() -> String {
    REGISTRY
        .iter()
        .map(|t| t.name)
        .collect::<Vec<_>>()
        .join(", ")
}

pub struct ToolOutput {
    pub output: String,
}

const SETTINGS: &[&str] = &["mac", "dns", "hostname", "ipv6"];

/// Validate write-tool args BEFORE approval, so bad args reject without ever
/// prompting or touching the system (rule 4, fail closed).
pub fn validate_write(name: &str, args: &Value, cfg: &Config) -> anyhow::Result<()> {
    match name {
        "enforce_privacy" => Ok(()),
        "set_privacy_setting" => {
            let setting = args.get("setting").and_then(|v| v.as_str());
            let enabled = args.get("enabled").and_then(|v| v.as_bool());
            match (setting, enabled) {
                (Some(s), Some(_)) if SETTINGS.contains(&s) => Ok(()),
                _ => anyhow::bail!(
                    "bad args: expected {{\"setting\": one of {SETTINGS:?}, \"enabled\": bool}}"
                ),
            }
        }
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
        "enforce_privacy" => {
            if dry_run {
                return Ok(ToolOutput {
                    output: "[dry-run] would call arkad EnforceAll() — no change made".into(),
                });
            }
            Ok(ToolOutput {
                output: backend.enforce_all().await?,
            })
        }
        "set_privacy_setting" => {
            let setting = args.get("setting").and_then(|v| v.as_str()).unwrap_or("");
            let enabled = args
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if dry_run {
                return Ok(ToolOutput {
                    output: format!("[dry-run] would set {setting}={enabled} — no change made"),
                });
            }
            Ok(ToolOutput {
                output: backend.set_privacy_setting(setting, enabled).await?,
            })
        }
        "restart_service" => {
            let unit = args.get("unit").and_then(|v| v.as_str()).unwrap_or("");
            if dry_run {
                return Ok(ToolOutput {
                    output: format!("[dry-run] would restart {unit} — no change made"),
                });
            }
            Ok(ToolOutput {
                output: backend.restart_service(unit).await?,
            })
        }
        other => anyhow::bail!("'{other}' is not a write tool"),
    }
}
