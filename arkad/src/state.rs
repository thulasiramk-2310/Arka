use std::sync::Arc;
use tokio::sync::RwLock;
use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};

pub type SharedState = Arc<RwLock<ArkadState>>;

#[derive(Debug, Clone)]
pub struct ArkadState {
    pub privacy_score: u8,
    pub mac_randomization: bool,
    pub dns_status: DnsStatus,
    pub hostname_privacy: bool,
    pub ipv6_privacy: bool,
    pub sandbox_status: SandboxStatus,
    pub browser_sandbox: BrowserSandbox,
    pub telemetry_blocked: bool,
    pub tracking_blocked: bool,
}

impl Default for ArkadState {
    fn default() -> Self {
        Self {
            privacy_score: 0,
            mac_randomization: false,
            dns_status: DnsStatus::Unknown("Initializing".into()),
            hostname_privacy: false,
            ipv6_privacy: false,
            sandbox_status: SandboxStatus::Unknown("Initializing".into()),
            browser_sandbox: BrowserSandbox::Unknown("Initializing".into()),
            // ArkaOS ships no telemetry or tracking by default
            telemetry_blocked: true,
            tracking_blocked: true,
        }
    }
}
