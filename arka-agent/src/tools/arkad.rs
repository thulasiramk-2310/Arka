//! arkad-backed read tool. Returns a human template string AND typed facts
//! (spec #1) so the answer verifier can check any claim the model makes. No
//! per-setting setter is called — arkad exposes none (rule 2).

use crate::backend::SystemBackend;
use crate::facts::{Fact, Status};

use super::ToolOutput;

fn onoff(b: bool) -> Status {
    if b {
        Status::On
    } else {
        Status::Off
    }
}
fn word(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

/// Standalone integers inside a value string (e.g. Quad9 IP -> [9,9,9,9]).
fn nums(s: &str) -> Vec<i64> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let before = start == 0 || !b[start - 1].is_ascii_alphanumeric();
            let after = i == b.len() || !b[i].is_ascii_alphanumeric();
            if before && after {
                if let Ok(n) = s[start..i].parse::<i64>() {
                    out.push(n);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

pub async fn system_status<B: SystemBackend>(backend: &B) -> anyhow::Result<ToolOutput> {
    let s = backend.privacy_status().await?;

    let output = format!(
        "privacy score:       {}/100\n\
         MAC randomization:   {}\n\
         DNS:                 {}\n\
         hostname privacy:    {}\n\
         IPv6 privacy:        {}\n\
         sandbox:             {}\n\
         browser sandbox:     {}",
        s.privacy_score,
        word(s.mac_randomization),
        s.dns_status,
        word(s.hostname_privacy),
        word(s.ipv6_privacy),
        s.sandbox_status,
        s.browser_sandbox,
    );

    let dns_on = s.dns_status.to_lowercase().contains("active")
        || s.dns_status.to_lowercase().contains("dot");
    let facts = vec![
        Fact::new("score", format!("{}/100", s.privacy_score)).num(s.privacy_score as i64),
        Fact::new("mac", format!("randomized ({})", word(s.mac_randomization)))
            .stat(onoff(s.mac_randomization)),
        Fact::new("dns", s.dns_status.clone())
            .nums(nums(&s.dns_status))
            .stat(if dns_on { Status::On } else { Status::Off }),
        Fact::new("hostname", format!("masked ({})", word(s.hostname_privacy)))
            .stat(onoff(s.hostname_privacy)),
        Fact::new(
            "ipv6",
            format!("privacy addresses ({})", word(s.ipv6_privacy)),
        )
        .stat(onoff(s.ipv6_privacy)),
        Fact::new("browser", s.browser_sandbox.clone()).stat(
            if s.sandbox_status.to_lowercase().contains("active") {
                Status::On
            } else {
                Status::Off
            },
        ),
    ];

    Ok(ToolOutput::with_facts(output, facts))
}
