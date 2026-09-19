//! arka-pulse-backed tool. READ-ONLY. Goes through `SystemBackend`, which uses
//! the in-process `arka_pulse` library on-device. Never touches
//! `arka_pulse::recover` — recovery stays dry-run/disabled (rule 8).

use crate::backend::SystemBackend;

use super::ToolOutput;

pub async fn pulse_health<B: SystemBackend>(backend: &B) -> anyhow::Result<ToolOutput> {
    let r = backend.pulse_health().await?;
    let cpu = r
        .cpu_util
        .map(|c| format!("{c:.0}%"))
        .unwrap_or_else(|| "n/a".into());
    let temp = r
        .temp_max
        .map(|t| format!("{t:.0}\u{00b0}C"))
        .unwrap_or_else(|| "n/a".into());
    let findings = if r.findings.is_empty() {
        "none".to_string()
    } else {
        r.findings.join("; ")
    };
    let output = format!(
        "health:    {}\n\
         CPU:       {}\n\
         memory:    {:.0}%\n\
         temp:      {}\n\
         findings:  {}",
        r.worst, cpu, r.mem_pct, temp, findings,
    );
    Ok(ToolOutput { output })
}
