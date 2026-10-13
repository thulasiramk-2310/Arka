mod command;
mod config;
mod reconcile;
mod enforcers;
mod error;
mod ipc;
pub mod log;
mod score;
mod state;

use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use ipc::ArkadIface;
use state::{ArkadState, SharedState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .without_time()
        .init();

    info!("arkad starting");
    log::log_event("system", "started", "ArkaOS privacy daemon started");

    let cfg = config::Config::load();

    let enforcers: Arc<Vec<Box<dyn enforcers::AsyncEnforcer>>> = Arc::new(vec![
        Box::new(enforcers::mac::MacEnforcer::new(&cfg)),
        Box::new(enforcers::dns::DnsEnforcer::new(&cfg)),
        Box::new(enforcers::hostname::HostnameEnforcer::new(&cfg)),
        Box::new(enforcers::ipv6::Ipv6Enforcer::new(&cfg)),
        Box::new(enforcers::firewall::FirewallEnforcer::new(&cfg)),
        Box::new(enforcers::sandbox::SandboxEnforcer),
    ]);

    let shared: SharedState = Arc::new(tokio::sync::RwLock::new(ArkadState::default()));

    let enforcement_lock = Arc::new(tokio::sync::Mutex::new(()));
    let failed = reconcile::reconcile(&enforcers, &shared, true).await;
    // Plain counts only: ArkaOS never presents privacy as a score.
    log::log_event("system", "ready", &match failed.len() {
        0 => "Startup check finished: every setting confirmed".to_string(),
        1 => "Startup check finished: 1 setting not confirmed".to_string(),
        n => format!("Startup check finished: {n} settings not confirmed"),
    });

    // Register D-Bus service
    let _conn = zbus::connection::Builder::system()?
        .name("org.arka.arkad")?
        .serve_at("/org/arka/arkad", ArkadIface {
            authorization_lock: tokio::sync::Mutex::new(()),
            state:     shared.clone(),
            enforcers: enforcers.clone(),
            enforcement_lock: enforcement_lock.clone(),
        })?
        .build()
        .await?;

    // Notify systemd we're ready. unset_env must stay false — unsetting
    // NOTIFY_SOCKET here would silence every later watchdog keepalive and
    // systemd would kill us on schedule (WatchdogSec) forever.
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]);

    info!(unconfirmed = failed.len(), "arkad ready");

    // Fixed, bounded retry cadence avoids spinning on small machines. Each
    // control is checked independently; healthy controls are not reapplied.
    let interval = Duration::from_secs(cfg.check_interval_secs.clamp(10, 60));
    // Right after boot a slow control (firewalld on a USB stick) can miss its
    // first check. Re-check quickly a few times so a transient miss clears in
    // seconds, not after a full interval. Healthy controls are not reapplied.
    let mut quick_rechecks = if failed.is_empty() { 0 } else { 4 };
    loop {
        if quick_rechecks > 0 {
            quick_rechecks -= 1;
            tokio::time::sleep(Duration::from_secs(5)).await;
        } else {
            tokio::time::sleep(interval).await;
        }
        let _guard = enforcement_lock.lock().await;
        reconcile::reconcile(&enforcers, &shared, false).await;
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
    }
}
