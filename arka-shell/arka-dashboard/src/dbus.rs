use std::sync::mpsc::Sender;

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};

use crate::state::{DashboardState, ReliabilityState, StateUpdate};

#[zbus::proxy(
    interface = "org.arka.arkad",
    default_service = "org.arka.arkad",
    default_path = "/org/arka/arkad",
    gen_blocking = false
)]
trait Arkad {
    #[zbus(property)]
    fn privacy_score(&self) -> zbus::Result<u8>;
    #[zbus(property)]
    fn dns_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn mac_randomization(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn hostname_privacy(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn ipv6_privacy(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn sandbox_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn browser_sandbox(&self) -> zbus::Result<String>;

    #[zbus(allow_interactive_auth)]
    fn enforce_all(&self) -> zbus::Result<()>;
}

/// Read-only client for arka-pulse's reliability surface. Mirrors the Arkad
/// proxy; every member is a property read — there is no mutating method to call.
#[zbus::proxy(
    interface = "org.arka.pulse",
    default_service = "org.arka.pulse",
    default_path = "/org/arka/pulse",
    gen_blocking = false
)]
trait Pulse {
    #[zbus(property)]
    fn sampled_at(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn health(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn summary(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn cpu_util(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn mem_pct(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn temp_max(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn predictions(&self) -> zbus::Result<Vec<(String, f64, f64, String)>>;
}

pub fn start_worker(tx: Sender<StateUpdate>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        rt.block_on(async move {
            if let Err(e) = worker_loop(tx).await {
                tracing::error!("dbus worker: {e}");
            }
        });
    });
}

async fn worker_loop(tx: Sender<StateUpdate>) -> zbus::Result<()> {
    loop {
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let conn = zbus::Connection::system().await?;
            let proxy = ArkadProxy::builder(&conn)
                .cache_properties(zbus::proxy::CacheProperties::No).build().await?;
            let pulse = PulseProxy::builder(&conn)
                .cache_properties(zbus::proxy::CacheProperties::No).build().await?;
            fetch_full(&proxy, &pulse, &tx).await
        }).await;
        if !matches!(result, Ok(Ok(()))) {
            // Replace previous success immediately; reconnect on the next poll.
            if tx.send(StateUpdate::Full(Box::default())).is_err() { return Ok(()); }
        }
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
}

async fn fetch_full(
    proxy: &ArkadProxy<'_>,
    pulse: &PulseProxy<'_>,
    tx: &Sender<StateUpdate>,
) -> zbus::Result<()> {
    let state = DashboardState {
        available:         true,
        privacy_score:     proxy.privacy_score().await?,
        dns_status:        DnsStatus::from(proxy.dns_status().await?),
        mac_randomization: proxy.mac_randomization().await?,
        hostname_privacy:  proxy.hostname_privacy().await?,
        ipv6_privacy:      proxy.ipv6_privacy().await?,
        sandbox_status:    SandboxStatus::from(proxy.sandbox_status().await?),
        browser_sandbox:   BrowserSandbox::from(proxy.browser_sandbox().await?),
        telemetry_blocked: true,
        tracking_blocked:  true,
        // Best-effort: if arka-pulse is unreachable, reliability stays the
        // default "unavailable" rather than failing the whole privacy fetch.
        reliability:       tokio::time::timeout(std::time::Duration::from_secs(1), fetch_reliability(pulse))
            .await.ok().and_then(Result::ok).unwrap_or_default(),
    };
    tx.send(StateUpdate::Full(Box::new(state)))
        .map_err(|_| zbus::Error::Failure("Dashboard closed".into()))?;
    Ok(())
}

async fn fetch_reliability(pulse: &PulseProxy<'_>) -> zbus::Result<ReliabilityState> {
    let sampled_at = pulse.sampled_at().await?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if sampled_at == 0 || sampled_at > now || now - sampled_at > 30 {
        return Err(zbus::Error::Failure("System health snapshot is stale".into()));
    }
    // Highest-probability prediction, if any — presented to the user as merely
    // *possible* instability, never a certain failure.
    let prediction = pulse
        .predictions()
        .await?
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(_domain, prob, _conf, summary)| (summary, prob));

    Ok(ReliabilityState {
        available: true,
        health: pulse.health().await?,
        summary: pulse.summary().await?,
        cpu_util: pulse.cpu_util().await?,
        mem_pct: pulse.mem_pct().await?,
        temp_max: pulse.temp_max().await?,
        prediction,
    })
}

pub fn call_enforce_all(tx: Sender<StateUpdate>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        let result = rt.block_on(async {
            let conn = zbus::Connection::system().await?;
            let proxy = ArkadProxy::new(&conn).await?;
            proxy.enforce_all().await?;
            Ok::<(), zbus::Error>(())
        });
        tx.send(StateUpdate::EnforceResult(result.map_err(|e| e.to_string()))).ok();
    });
}
