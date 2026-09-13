use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};

/// Machine-health state read from arka-pulse's `org.arka.pulse` (read-only).
/// `available` is false when the daemon isn't reachable, so the UI can say so
/// rather than showing a stale or fake verdict. `health`/`summary` come from
/// the deterministic DETECT stage; `prediction` is a HEURISTIC trend note.
#[derive(Clone, Debug)]
pub struct ReliabilityState {
    pub available: bool,
    pub health: String,   // OK / WARN / CRIT
    pub summary: String,
    pub cpu_util: f64,     // -1.0 = unknown
    pub mem_pct: f64,
    pub temp_max: f64,     // -1.0 = unknown
    /// Top prediction as (summary, probability) if any — presented as *possible*.
    pub prediction: Option<(String, f64)>,
}

impl Default for ReliabilityState {
    fn default() -> Self {
        Self {
            available: false,
            health: "…".into(),
            summary: "Checking system health…".into(),
            cpu_util: -1.0,
            mem_pct: -1.0,
            temp_max: -1.0,
            prediction: None,
        }
    }
}

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
    pub reliability: ReliabilityState,
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
            reliability: ReliabilityState::default(),
        }
    }
}

#[derive(Debug)]
pub enum StateUpdate {
    Full(Box<DashboardState>),
    EnforceResult(Result<(), String>),
}
