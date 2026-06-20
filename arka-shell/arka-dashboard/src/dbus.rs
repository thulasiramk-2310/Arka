use std::sync::mpsc::Sender;

use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
use futures::StreamExt;

use crate::state::{DashboardState, StateUpdate};

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

    fetch_full(&proxy, &tx).await.ok();

    let mut s_score   = proxy.receive_privacy_score_changed().await;
    let mut s_dns     = proxy.receive_dns_status_changed().await;
    let mut s_mac     = proxy.receive_mac_randomization_changed().await;
    let mut s_host    = proxy.receive_hostname_privacy_changed().await;
    let mut s_ipv6    = proxy.receive_ipv6_privacy_changed().await;
    let mut s_sandbox = proxy.receive_sandbox_status_changed().await;
    let mut s_browser = proxy.receive_browser_sandbox_changed().await;

    loop {
        tokio::select! {
            v = s_score.next()   => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
            v = s_dns.next()     => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
            v = s_mac.next()     => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
            v = s_host.next()    => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
            v = s_ipv6.next()    => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
            v = s_sandbox.next() => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
            v = s_browser.next() => { if v.is_none() { break; } fetch_full(&proxy, &tx).await.ok(); }
        }
    }
    Ok(())
}

async fn fetch_full(proxy: &ArkadProxy<'_>, tx: &Sender<StateUpdate>) -> zbus::Result<()> {
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
    };
    tx.send(StateUpdate::Full(Box::new(state))).ok();
    Ok(())
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
