mod config;
mod enforcers;
mod error;
mod ipc;
pub mod log;
mod score;
mod state;

use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

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
        Box::new(enforcers::sandbox::SandboxEnforcer),
    ]);

    let shared: SharedState = Arc::new(tokio::sync::RwLock::new(ArkadState::default()));

    // Initial enforcement pass
    for e in enforcers.iter() {
        if let Err(err) = e.enforce().await {
            error!(enforcer = e.name(), %err, "initial enforce failed");
        }
    }

    // Collect initial state
    for e in enforcers.iter() {
        e.update_state(&shared).await;
    }
    {
        let mut s = shared.write().await;
        s.privacy_score = score::compute(&s);
    }

    let initial_score = shared.read().await.privacy_score;
    log::log_event("system", "ready", &format!("Privacy score: {initial_score}/100 — all controls active"));

    // Register D-Bus service
    let _conn = zbus::connection::Builder::system()?
        .name("org.arka.arkad")?
        .serve_at("/org/arka/arkad", ArkadIface {
            state:     shared.clone(),
            enforcers: enforcers.clone(),
        })?
        .build()
        .await?;

    // Notify systemd we're ready
    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);

    info!(score = initial_score, "arkad ready");

    // Watch loop — verify state every interval, re-enforce on drift
    let interval = Duration::from_secs(cfg.check_interval_secs);
    loop {
        tokio::time::sleep(interval).await;

        let prev_score = shared.read().await.privacy_score;

        for e in enforcers.iter() {
            e.update_state(&shared).await;
        }
        let new_score = {
            let mut s = shared.write().await;
            s.privacy_score = score::compute(&s);
            s.privacy_score
        };

        if new_score < prev_score {
            info!(prev = prev_score, now = new_score, "drift detected, re-enforcing");
            log::log_event(
                "system",
                "drift",
                &format!("Privacy drift detected (score {prev_score}→{new_score}) — re-enforcing"),
            );
            for e in enforcers.iter() {
                if let Err(err) = e.enforce().await {
                    error!(enforcer = e.name(), %err, "re-enforce failed");
                }
            }
            for e in enforcers.iter() {
                e.update_state(&shared).await;
            }
            let mut s = shared.write().await;
            s.privacy_score = score::compute(&s);
            let recovered = s.privacy_score;
            log::log_event("system", "recovered", &format!("Re-enforcement complete — score restored to {recovered}/100"));
        }

        // Watchdog keepalive
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
    }
}
