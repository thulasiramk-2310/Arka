use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};

#[derive(Clone, Debug)]
pub struct DashboardState {
    pub privacy_score: u8,
    pub dns_status: DnsStatus,
    pub mac_randomization: bool,
    pub hostname_privacy: bool,
    pub ipv6_privacy: bool,
    pub sandbox_status: SandboxStatus,
    pub browser_sandbox: BrowserSandbox,
    pub telemetry_blocked: bool,
    pub tracking_blocked: bool,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            privacy_score: 0,
            dns_status: DnsStatus::Unknown("…".into()),
            mac_randomization: false,
            hostname_privacy: false,
            ipv6_privacy: false,
            sandbox_status: SandboxStatus::Unknown("…".into()),
            browser_sandbox: BrowserSandbox::Unknown("…".into()),
            telemetry_blocked: true,
            tracking_blocked: true,
        }
    }
}

#[derive(Debug)]
pub enum StateUpdate {
    Full(Box<DashboardState>),
    EnforceResult(Result<(), String>),
}
