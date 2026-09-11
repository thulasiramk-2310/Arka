//! Calibration recorder — append one JSON line per health sample to a log file.
//!
//! Stage 3 of the evidence ladder (`docs/RELIABILITY-ARKA-PULSE.md`) needs real
//! telemetry captured over long real-world sessions, so prediction quality
//! (precision, false-positive rate, lead time) can be measured *after the fact*
//! against what actually happened. This writer is that capture.
//!
//! Three properties keep it honest and safe:
//! - **Read-only-adjacent.** It only ever appends to one operator-named file; it
//!   never influences the engine or the system being observed.
//! - **Best-effort.** A failed write is reported once and dropped — monitoring
//!   continues. A calibration log must never be able to crash the monitor.
//! - **Library-free.** It lives in the binary, not the crate, so the zero-dep
//!   reliability core stays free of any serialization concern. JSON is emitted
//!   by hand for the same reason `main.rs` formats its own output.
//!
//! Format: JSONL (one JSON object per line). The first line is a `"meta"`
//! record (schema version, start time, host identity); every later line is a
//! `"sample"` record. Analysis is deliberately not here — capture first, then
//! measure once real data exists.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};

use arka_pulse::service::HealthSnapshot;

/// Append-only JSONL sink for health samples.
pub struct Recorder {
    file: File,
    path: String,
}

impl Recorder {
    /// Open (creating if absent) the log for appending. Errors propagate: when
    /// the operator asked to record, a log we cannot open is a hard failure —
    /// silently monitoring without capturing would waste the session.
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Recorder {
            file,
            path: path.to_string(),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// One-time metadata line: schema version, start time, and host identity, so
    /// logs from different machines stay distinguishable later ("stability
    /// across machines" in the calibration plan).
    pub fn write_meta(&mut self, start_unix: f64) {
        let host = read_trim("/proc/sys/kernel/hostname");
        let kernel = read_trim("/proc/sys/kernel/osrelease");
        let line = format!(
            "{{\"type\":\"meta\",\"schema\":1,\"start\":{:.3},\"host\":{},\"kernel\":{}}}",
            start_unix,
            jstr(&host),
            jstr(&kernel)
        );
        self.write_line(&line);
    }

    /// Append one sample: raw telemetry plus every finding and prediction, so a
    /// later analysis can line up "what was predicted" against "what happened".
    pub fn record(&mut self, t: f64, s: &HealthSnapshot) {
        let tel = &s.telemetry;
        let swap = if tel.memory.swap_total_kb > 0 {
            Some(tel.memory.swap_used_pct())
        } else {
            None
        };

        let mut line = String::with_capacity(512);
        line.push_str("{\"type\":\"sample\"");
        line.push_str(&format!(",\"t\":{:.3}", t));
        line.push_str(&format!(",\"worst\":{}", jstr(s.worst.label())));
        line.push_str(&format!(
            ",\"load1\":{:.2},\"load5\":{:.2},\"load15\":{:.2}",
            tel.load1, tel.load5, tel.load15
        ));
        line.push_str(&format!(",\"cpu_util\":{}", jopt(tel.cpu_util)));
        line.push_str(&format!(",\"mem_pct\":{:.2}", tel.memory.used_pct()));
        line.push_str(&format!(",\"swap_pct\":{}", jopt(swap)));
        line.push_str(&format!(
            ",\"psi_cpu10\":{},\"psi_mem10\":{},\"psi_io10\":{}",
            jopt(tel.psi.cpu_some10),
            jopt(tel.psi.mem_some10),
            jopt(tel.psi.io_some10)
        ));
        line.push_str(&format!(",\"temp_max\":{}", jopt(tel.thermal.max_c())));

        line.push_str(",\"findings\":[");
        for (i, f) in s.findings.iter().enumerate() {
            if i > 0 {
                line.push(',');
            }
            line.push_str(&format!(
                "{{\"domain\":{},\"sev\":{},\"summary\":{},\"evidence\":{}}}",
                jstr(f.domain),
                jstr(f.severity.label()),
                jstr(&f.summary),
                jstr(&f.evidence)
            ));
        }
        line.push(']');

        line.push_str(",\"predictions\":[");
        for (i, p) in s.predictions.iter().enumerate() {
            if i > 0 {
                line.push(',');
            }
            line.push_str(&format!(
                "{{\"domain\":{},\"prob\":{:.4},\"confidence\":{:.4},\"lead_s\":{:.1},\"summary\":{},\"evidence\":{}}}",
                jstr(p.domain),
                p.probability,
                p.confidence,
                p.lead_time_secs,
                jstr(&p.summary),
                jstr(&p.evidence)
            ));
        }
        line.push(']');
        line.push('}');

        self.write_line(&line);
    }

    fn write_line(&mut self, line: &str) {
        if let Err(e) = writeln!(self.file, "{line}") {
            eprintln!("record: write to {} failed: {e} (continuing)", self.path);
        }
    }
}

/// JSON-encode an `Option<f64>` as a fixed-precision number or `null`.
fn jopt(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.3}"),
        None => "null".to_string(),
    }
}

/// JSON-encode a string: quoted, with the mandatory escapes. Findings carry
/// controlled text today, but escaping properly keeps the log valid JSON no
/// matter what a future detector puts in a summary.
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn read_trim(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jopt_formats_number_or_null() {
        assert_eq!(jopt(Some(41.0)), "41.000");
        assert_eq!(jopt(None), "null");
    }

    #[test]
    fn jstr_escapes_control_and_quotes() {
        assert_eq!(jstr("ok"), "\"ok\"");
        assert_eq!(jstr("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(jstr("line\nbreak"), "\"line\\nbreak\"");
        // C0 control char below the named escapes gets \u00XX form.
        assert_eq!(jstr("\u{01}"), "\"\\u0001\"");
    }
}
