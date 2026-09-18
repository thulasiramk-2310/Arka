//! arkad-backed tools.
//!
//! Phase 2 wires these to the real `org.arka.arkad` interface on the SYSTEM bus
//! (path `/org/arka/arkad`), reading the properties verified in Phase 0:
//!   PrivacyScore(u8) · MacRandomization(bool) · DnsStatus(String)
//!   HostnamePrivacy(bool) · Ipv6Privacy(bool) · SandboxStatus(String)
//!   BrowserSandbox(String)
//! and, for the write path, the one real method: EnforceAll().
//!
//! There is intentionally NO per-setting setter here, because arkad exposes
//! none (see the TODOs in the crate docs). Do not invent one.

use serde_json::Value;

use super::ToolOutput;

pub async fn system_status(_args: &Value) -> anyhow::Result<ToolOutput> {
    // TODO(phase2): open a zbus system-bus proxy for org.arka.arkad and read
    // the seven properties, then format them here.
    Ok(ToolOutput {
        output: "TODO(phase2): read org.arka.arkad properties \
                 (PrivacyScore, MacRandomization, DnsStatus, HostnamePrivacy, \
                 Ipv6Privacy, SandboxStatus, BrowserSandbox) over the system bus"
            .into(),
    })
}
