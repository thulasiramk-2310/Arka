//! arkad-backed tools. Reads go through `SystemBackend` (Phase-0 properties).
//! No per-setting setter exists in arkad, so none is called here (rule 2).

use crate::backend::SystemBackend;

use super::ToolOutput;

fn onoff(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
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
        onoff(s.mac_randomization),
        s.dns_status,
        onoff(s.hostname_privacy),
        onoff(s.ipv6_privacy),
        s.sandbox_status,
        s.browser_sandbox,
    );
    Ok(ToolOutput { output })
}
