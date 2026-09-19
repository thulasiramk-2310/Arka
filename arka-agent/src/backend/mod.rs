//! The system backend abstraction. Tools depend on `SystemBackend`, never on
//! zbus or arka-pulse directly, so tests run against `MockBackend` (fixed,
//! realistic state) with no daemon, and on-device uses `DbusBackend`.
//!
//! Phase 2 defines the READ surface. Phase 3 extends this trait with the write
//! methods (enforce_all / set_privacy_setting / restart_service).

pub mod dbus;

use serde::Serialize;

/// Snapshot of arkad's privacy state — one field per Phase-0 property.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PrivacyStatus {
    pub privacy_score: u8,
    pub mac_randomization: bool,
    pub dns_status: String,
    pub hostname_privacy: bool,
    pub ipv6_privacy: bool,
    pub sandbox_status: String,
    pub browser_sandbox: String,
}

/// A read-only health summary derived from arka-pulse.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PulseReport {
    /// worst current severity: OK / INFO / WARN / CRIT
    pub worst: String,
    /// "domain: summary (evidence)" lines
    pub findings: Vec<String>,
    pub cpu_util: Option<f64>,
    pub mem_pct: f64,
    pub temp_max: Option<f64>,
}

/// Everything the agent's tools are allowed to ask the system to do. Read-only
/// in Phase 2. `#[allow(async_fn_in_trait)]`: static dispatch only (tools/agent
/// are generic over `B: SystemBackend`).
#[allow(async_fn_in_trait)]
pub trait SystemBackend {
    async fn privacy_status(&self) -> anyhow::Result<PrivacyStatus>;
    async fn pulse_health(&self) -> anyhow::Result<PulseReport>;
}

/// Deterministic in-memory backend for tests — a healthy DP1 machine.
pub struct MockBackend {
    pub status: PrivacyStatus,
    pub pulse: PulseReport,
}

impl MockBackend {
    pub fn healthy() -> Self {
        MockBackend {
            status: PrivacyStatus {
                privacy_score: 100,
                mac_randomization: true,
                dns_status: "DoT active (Quad9 9.9.9.9)".into(),
                hostname_privacy: true,
                ipv6_privacy: true,
                sandbox_status: "active".into(),
                browser_sandbox: "bubblewrap".into(),
            },
            pulse: PulseReport {
                worst: "OK".into(),
                findings: vec![],
                cpu_util: Some(27.0),
                mem_pct: 38.0,
                temp_max: Some(51.0),
            },
        }
    }
}

impl SystemBackend for MockBackend {
    async fn privacy_status(&self) -> anyhow::Result<PrivacyStatus> {
        Ok(self.status.clone())
    }
    async fn pulse_health(&self) -> anyhow::Result<PulseReport> {
        Ok(self.pulse.clone())
    }
}
