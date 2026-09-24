//! arka-pulse-backed read tool. READ-ONLY. Returns a human template string AND
//! typed facts (spec #1). Never touches `arka_pulse::recover` (rule 8).

use crate::backend::SystemBackend;
use crate::facts::Fact;

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

    let mut facts = vec![Fact::new("health", r.worst.clone())];
    if let Some(c) = r.cpu_util {
        facts.push(Fact::new("cpu", format!("{c:.0}%")).num(c as i64));
    }
    facts.push(Fact::new("mem", format!("{:.0}%", r.mem_pct)).num(r.mem_pct as i64));
    if let Some(t) = r.temp_max {
        facts.push(Fact::new("temp", format!("{t:.0}\u{00b0}C")).num(t as i64));
    }

    Ok(ToolOutput::with_facts(output, facts))
}
