//! On-device backend: real `org.arka.arkad` reads/writes over the system bus,
//! and in-process `arka_pulse` health. Compiled everywhere, exercised only by
//! `--ignored` live tests and on the laptop — never in the mock test path.
//!
//! Only the Phase-0-verified surface is used. arkad exposes no per-setting
//! setter, so `set_privacy_setting` returns a clear "not implemented in arkad"
//! error rather than inventing a method (rule 2).

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

    async fn proxy(&self) -> anyhow::Result<zbus::Proxy<'static>> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| anyhow::anyhow!("connect system bus: {e}"))?;
        zbus::Proxy::new(&conn, ARKAD_DEST, ARKAD_PATH, ARKAD_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("open arkad proxy: {e}"))
    }
}

impl SystemBackend for DbusBackend {
    async fn privacy_status(&self) -> anyhow::Result<PrivacyStatus> {
        let proxy = self.proxy().await?;
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

    async fn enforce_all(&self) -> anyhow::Result<String> {
        let proxy = self.proxy().await?;
        // The one real arkad method (Phase 0).
        proxy
            .call_method("EnforceAll", &())
            .await
            .map_err(|e| anyhow::anyhow!("arkad EnforceAll: {e}"))?;
        Ok("arkad re-applied all privacy enforcers (EnforceAll)".into())
    }

    async fn restart_service(&self, unit: &str) -> anyhow::Result<String> {
        // The unit is already allow-list-checked by the tool layer. Fixed argv,
        // never a shell string.
        let out = std::process::Command::new("systemctl")
            .args(["restart", unit])
            .output()
            .map_err(|e| anyhow::anyhow!("run systemctl: {e}"))?;
        if out.status.success() {
            Ok(format!("restarted {unit}"))
        } else {
            anyhow::bail!(
                "systemctl restart {unit} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )
        }
    }
}
