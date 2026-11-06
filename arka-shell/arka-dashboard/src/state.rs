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
    /// When arka-pulse took this sample (seconds since 1970); 0 = unknown. Shown on the Health page.
    pub sampled_at: u64,
}

impl Default for ReliabilityState {
    fn default() -> Self {
        Self {
            available: false,
            sampled_at: 0,
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
    pub available: bool,
    pub dns_status: DnsStatus,
    pub mac_randomization: bool,
    pub hostname_privacy: bool,
    pub ipv6_privacy: bool,
    pub firewall: bool,
    pub sandbox_status: SandboxStatus,
    pub browser_sandbox: BrowserSandbox,
    /// Unix seconds of arkad's last completed check (0 = never/unknown).
    pub last_check: u64,
    pub reliability: ReliabilityState,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            available: false,
            dns_status: DnsStatus::Unknown("…".into()),
            mac_randomization: false,
            hostname_privacy: false,
            ipv6_privacy: false,
            firewall: false,
            sandbox_status: SandboxStatus::Unknown("…".into()),
            browser_sandbox: BrowserSandbox::Unknown("…".into()),
            last_check: 0,
            reliability: ReliabilityState::default(),
        }
    }
}

impl DashboardState {
    /// Match the tray and desktop widget: strictly less than three 30 s polls.
    pub fn check_is_fresh_at(&self, now: u64) -> bool {
        self.available && self.last_check > 0 && self.last_check <= now
            && now - self.last_check < 90
    }
}

#[cfg(test)]
mod tests {
    use super::DashboardState;

    #[test]
    fn freshness_rejects_unknown_future_and_ninety_second_old_checks() {
        let mut s = DashboardState { available: true, last_check: 1_000, ..Default::default() };
        assert!(s.check_is_fresh_at(1_000));
        assert!(s.check_is_fresh_at(1_089));
        assert!(!s.check_is_fresh_at(1_090));
        assert!(!s.check_is_fresh_at(999));
        s.last_check = 0;
        assert!(!s.check_is_fresh_at(1_000));
        s.last_check = 1_000;
        s.available = false;
        assert!(!s.check_is_fresh_at(1_000));
    }
}

#[derive(Debug)]
pub enum StateUpdate {
    Full(Box<DashboardState>),
    EnforceResult(Result<(), String>),
    /// `bootc status` parsed, or why it could not be read.
    /// The status and the time of the file it was read from.
    Updates(Result<(arka_update::status::UpdateStatus, std::time::SystemTime), String>),
}
