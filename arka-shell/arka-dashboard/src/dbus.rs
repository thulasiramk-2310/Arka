use std::sync::mpsc::Sender;

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
use futures::StreamExt;

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
    let conn = zbus::Connection::system().await?;
    let proxy = ArkadProxy::new(&conn).await?;
    // Proxy creation is lazy and does not require the service to be running, so
    // the dashboard still works if arka-pulse isn't up yet (reads just fail and
    // reliability shows as unavailable).
    let pulse = PulseProxy::new(&conn).await?;

    fetch_full(&proxy, &pulse, &tx).await.ok();

    let mut s_score   = proxy.receive_privacy_score_changed().await;
    let mut s_dns     = proxy.receive_dns_status_changed().await;
    let mut s_mac     = proxy.receive_mac_randomization_changed().await;
    let mut s_host    = proxy.receive_hostname_privacy_changed().await;
    let mut s_ipv6    = proxy.receive_ipv6_privacy_changed().await;
    let mut s_sandbox = proxy.receive_sandbox_status_changed().await;
    let mut s_browser = proxy.receive_browser_sandbox_changed().await;

    // arka-pulse properties don't emit change signals (the daemon mutates its
    // snapshot directly), so poll them on a timer. The daemon samples every
    // ~10s; polling at 4s keeps the health view fresh without busy-reading.
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(4));

    loop {
        tokio::select! {
            v = s_score.next()   => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            v = s_dns.next()     => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            v = s_mac.next()     => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            v = s_host.next()    => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            v = s_ipv6.next()    => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            v = s_sandbox.next() => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            v = s_browser.next() => { if v.is_none() { break; } fetch_full(&proxy, &pulse, &tx).await.ok(); }
            _ = tick.tick()      => { fetch_full(&proxy, &pulse, &tx).await.ok(); }
        }
    }
    Ok(())
}

async fn fetch_full(
    proxy: &ArkadProxy<'_>,
    pulse: &PulseProxy<'_>,
    tx: &Sender<StateUpdate>,
) -> zbus::Result<()> {
    let state = DashboardState {
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
        reliability:       fetch_reliability(pulse).await.unwrap_or_default(),
    };
    tx.send(StateUpdate::Full(Box::new(state))).ok();
    Ok(())
}

async fn fetch_reliability(pulse: &PulseProxy<'_>) -> zbus::Result<ReliabilityState> {
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
