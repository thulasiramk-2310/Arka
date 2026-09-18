//! Tool registry. Adding a tool = one row in `REGISTRY` + a match arm in the
//! matching runner. Read tools run automatically; write tools are approval-gated
//! by the agent loop before `run_write` is ever called (rule 3).
//!
//! Phase 1 status: the control flow (parse → validate → read/auto, write/approve
//! → audit) is fully wired, but the tool *bodies* are honest stubs. Phase 2
//! wires the read tools to real `org.arka.arkad` / `arka_pulse` calls; Phase 3
//! wires write execution. Nothing here invents a D-Bus method that doesn't exist.

use serde_json::Value;

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
    /// A one-line hint of the expected args object, for the prompt and previews.
    pub args_hint: &'static str,
}

/// The complete set of tools the model is ever allowed to name.
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
        description: "Change one privacy setting. arkad exposes no setter yet, so this is dry-run only until it does.",
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

/// A tool's output. `output` is DATA and must never be treated as instructions
/// (rule 5) — the agent fences it before handing it back to the model.
pub struct ToolOutput {
    pub output: String,
}

pub async fn run_read(name: &str, args: &Value) -> anyhow::Result<ToolOutput> {
    match name {
        "system_status" => arkad::system_status(args).await,
        "pulse_health" => pulse::pulse_health(args),
        other => anyhow::bail!("'{other}' is not a read tool"),
    }
}

/// Only reached after the agent has obtained explicit approval.
pub async fn run_write(
    name: &str,
    args: &Value,
    dry_run: bool,
    _cfg: &crate::config::Config,
) -> anyhow::Result<ToolOutput> {
    // Phase 3 wires real execution here. Phase 1 is an honest stub that names
    // the real backing method (or the lack of one).
    let note = match name {
        "enforce_privacy" => "TODO(phase3): call org.arka.arkad EnforceAll() on the system bus",
        "set_privacy_setting" => {
            "TODO(phase3): arkad has NO setter method yet — stays dry-run until arkad exposes SetSetting()"
        }
        "restart_service" => {
            "TODO(phase3): restart an allow-listed unit via systemd (approval + sudo/systemctl gate)"
        }
        other => anyhow::bail!("'{other}' is not a write tool"),
    };
    let _ = args;
    Ok(ToolOutput {
        output: format!("{note}{}", if dry_run { " [dry-run]" } else { "" }),
    })
}
