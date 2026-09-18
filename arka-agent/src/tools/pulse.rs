//! arka-pulse-backed tools.
//!
//! Phase 2 wires this to the in-process library (chosen over the D-Bus proxy):
//!   let mut e = arka_pulse::service::PulseEngine::new();
//!   let snap = e.health()?;   // HealthSnapshot { worst, findings, predictions, telemetry, .. }
//! and summarises worst severity + findings + CPU/mem/temp.
//!
//! This tool is READ-ONLY. It must never touch `arka_pulse::recover` — recovery
//! stays dry-run/disabled (rule 8).

use serde_json::Value;

use super::ToolOutput;

pub fn pulse_health(_args: &Value) -> anyhow::Result<ToolOutput> {
    // TODO(phase2): call arka_pulse PulseEngine::health() and format
    // worst/findings/telemetry. Read-only; never call recover::*.
    Ok(ToolOutput {
        output: "TODO(phase2): call arka_pulse PulseEngine::health() and summarise \
                 worst severity, findings, and CPU/mem/temp telemetry (read-only)"
            .into(),
    })
}
