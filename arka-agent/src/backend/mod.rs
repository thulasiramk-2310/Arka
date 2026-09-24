//! The system backend abstraction. Tools depend on `SystemBackend`, never on
//! zbus or arka-pulse directly, so tests run against `MockBackend` (fixed,
//! realistic state) with no daemon, and on-device uses `DbusBackend`.
//!
//! Read methods observe only. Write methods change the system and are only ever
//! reached through the agent loop's approval gate (rule 3). Dry-run is handled
//! one level up (in `tools::run_write`) and must never call these write methods.

pub mod dbus;

use std::sync::atomic::{AtomicUsize, Ordering};

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
    pub worst: String,
    pub findings: Vec<String>,
    pub cpu_util: Option<f64>,
    pub mem_pct: f64,
    pub temp_max: Option<f64>,
}

/// Everything the agent's tools are allowed to ask the system to do.
///
/// `#[allow(async_fn_in_trait)]`: static dispatch only (tools/agent are generic
/// over `B: SystemBackend`).
#[allow(async_fn_in_trait)]
pub trait SystemBackend {
    // ── read ──
    async fn privacy_status(&self) -> anyhow::Result<PrivacyStatus>;
    async fn pulse_health(&self) -> anyhow::Result<PulseReport>;

    // ── write (reached only after approval; never called in dry-run) ──
    /// Re-apply every privacy enforcer — arkad `EnforceAll()`.
    async fn enforce_all(&self) -> anyhow::Result<String>;
    /// Restart an (already allow-list-checked) systemd unit.
    async fn restart_service(&self, unit: &str) -> anyhow::Result<String>;
}

/// Deterministic in-memory backend for tests — a healthy DP1 machine. Counts
/// write calls so a test can prove dry-run never reached the backend.
pub struct MockBackend {
    pub status: PrivacyStatus,
    pub pulse: PulseReport,
    pub enforce_calls: AtomicUsize,
    pub restart_calls: AtomicUsize,
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
            enforce_calls: AtomicUsize::new(0),
            restart_calls: AtomicUsize::new(0),
        }
    }
    /// TEST-ONLY: a healthy backend whose DNS status string leaks fake
    /// secrets, to prove redaction end to end (they must reach neither the
    /// final answer nor the audit log). Not compiled into a release build.
    #[cfg(test)]
    pub fn leaking_secret() -> Self {
        let mut b = Self::healthy();
        b.status.dns_status =
            "DoT active (Quad9 9.9.9.9) psk=Hunter2Leak token: ghp_FAKE0123456789abcdefLEAK".into();
        b
    }
    pub fn writes_total(&self) -> usize {
        self.enforce_calls.load(Ordering::SeqCst) + self.restart_calls.load(Ordering::SeqCst)
    }
}

impl SystemBackend for MockBackend {
    async fn privacy_status(&self) -> anyhow::Result<PrivacyStatus> {
        Ok(self.status.clone())
    }
    async fn pulse_health(&self) -> anyhow::Result<PulseReport> {
        Ok(self.pulse.clone())
    }
    async fn enforce_all(&self) -> anyhow::Result<String> {
        self.enforce_calls.fetch_add(1, Ordering::SeqCst);
        Ok("re-applied all privacy enforcers".into())
    }
    async fn restart_service(&self, unit: &str) -> anyhow::Result<String> {
        self.restart_calls.fetch_add(1, Ordering::SeqCst);
        Ok(format!("restarted {unit}"))
    }
}
