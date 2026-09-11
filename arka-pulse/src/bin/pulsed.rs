//! arka-pulsed — the read-only D-Bus daemon for arka-pulse.
//!
//! Runs the deterministic `PulseEngine` on an interval and publishes the latest
//! snapshot on the **system bus** as `org.arka.pulse`, mirroring arkad. It reads
//! `/proc` + `/sys` and changes nothing: recovery stays out of this process
//! entirely, so the daemon is safe to run always-on as a background service.
//!
//! Built only with `--features dbus` (see Cargo.toml); the default build stays
//! zero-dependency.
//!
//! Usage: arka-pulsed [--interval SECONDS]

use std::sync::Arc;
use std::time::Duration;

use arka_pulse::ipc::{PulseIface, PulseSnapshot, SharedPulse};
use arka_pulse::service::{PulseEngine, ReliabilityService};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let interval_secs = std::env::args()
        .skip_while(|a| a != "--interval")
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10)
        .max(1);

    let shared: SharedPulse = Arc::new(RwLock::new(PulseSnapshot::default()));

    // Publish read-only on the system bus, exactly as arkad does. Holding the
    // connection alive for the process lifetime keeps the name owned.
    let _conn = zbus::connection::Builder::system()?
        .name("org.arka.pulse")?
        .serve_at("/org/arka/pulse", PulseIface { state: shared.clone() })?
        .build()
        .await?;

    eprintln!("arka-pulsed: serving org.arka.pulse (read-only), interval {interval_secs}s");

    // The engine samples synchronously (microsecond /proc + /sys reads), so it
    // runs inline on the reactor between sleeps rather than needing a blocking
    // pool. new() primed the CPU meter, so the first sample already has util.
    let mut engine = PulseEngine::new();
    let interval = Duration::from_secs(interval_secs);
    loop {
        match engine.health() {
            Ok(snap) => *shared.write().await = PulseSnapshot::from_health(&snap),
            Err(e) => eprintln!("arka-pulsed: monitor error: {e}"),
        }
        tokio::time::sleep(interval).await;
    }
}
