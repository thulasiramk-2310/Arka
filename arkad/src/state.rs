use std::sync::Arc;
use tokio::sync::RwLock;
use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};

pub type SharedState = Arc<RwLock<ArkadState>>;

impl ArkadState {
    pub fn control_healthy(&self, name: &str) -> bool {
        match name {
            "mac" => self.mac_randomization,
            "dns" => self.dns_status.is_encrypted(),
            "hostname" => self.hostname_privacy,
            "ipv6" => self.ipv6_privacy,
            "sandbox" => self.sandbox_status.is_active() && self.browser_sandbox.is_isolated(),
            _ => false,
        }
    }

    pub fn invalidate(&mut self, name: &str) {
        match name {
            "mac" => self.mac_randomization = false,
            "dns" => self.dns_status = DnsStatus::Error,
            "hostname" => self.hostname_privacy = false,
            "ipv6" => self.ipv6_privacy = false,
            "sandbox" => {
                self.sandbox_status = SandboxStatus::Inactive;
                self.browser_sandbox = BrowserSandbox::None;
            }
            _ => (),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArkadState {
    /// Unix seconds of the last completed reconcile; zero until one completes.
    pub last_check: u64,
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
            last_check: 0,
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
