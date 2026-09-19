//! On-device backend: real `org.arka.arkad` reads over the system bus, and
//! in-process `arka_pulse` health. Compiled everywhere, but only exercised by
//! `--ignored` live tests and on the laptop — never in the mock test path.
//!
//! Only the Phase-0-verified surface is used. There is intentionally no
//! per-setting setter: arkad exposes none, so Phase 3's `set_privacy_setting`
//! will return a clear "not implemented in arkad" error here (rule 2).

use super::{PrivacyStatus, PulseReport, SystemBackend};

const ARKAD_DEST: &str = "org.arka.arkad";
const ARKAD_PATH: &str = "/org/arka/arkad";
const ARKAD_IFACE: &str = "org.arka.arkad";

#[derive(Default)]
pub struct DbusBackend;

impl DbusBackend {
    pub fn new() -> Self {
        DbusBackend
    }
}

impl SystemBackend for DbusBackend {
    async fn privacy_status(&self) -> anyhow::Result<PrivacyStatus> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| anyhow::anyhow!("connect system bus: {e}"))?;
        let proxy = zbus::Proxy::new(&conn, ARKAD_DEST, ARKAD_PATH, ARKAD_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("open arkad proxy: {e}"))?;

        // Property names are the PascalCase of the daemon's methods (Phase 0).
        Ok(PrivacyStatus {
            privacy_score: proxy.get_property("PrivacyScore").await?,
            mac_randomization: proxy.get_property("MacRandomization").await?,
            dns_status: proxy.get_property("DnsStatus").await?,
            hostname_privacy: proxy.get_property("HostnamePrivacy").await?,
            ipv6_privacy: proxy.get_property("Ipv6Privacy").await?,
            sandbox_status: proxy.get_property("SandboxStatus").await?,
            browser_sandbox: proxy.get_property("BrowserSandbox").await?,
        })
    }

    async fn pulse_health(&self) -> anyhow::Result<PulseReport> {
        // In-process (chosen in Phase 0): deterministic, no daemon needed, and
        // strictly read-only. NEVER touch arka_pulse::recover (rule 8).
        use arka_pulse::service::{PulseEngine, ReliabilityService};

        let mut engine = PulseEngine::new();
        let snap = engine
            .health()
            .map_err(|e| anyhow::anyhow!("arka_pulse health: {e}"))?;

        let findings = snap
            .findings
            .iter()
            .map(|f| format!("{}: {} ({})", f.domain, f.summary, f.evidence))
            .collect();

        Ok(PulseReport {
            worst: snap.worst.label().to_string(),
            findings,
            cpu_util: snap.telemetry.cpu_util,
            mem_pct: snap.telemetry.memory.used_pct(),
            temp_max: snap.telemetry.thermal.max_c(),
        })
    }
}
