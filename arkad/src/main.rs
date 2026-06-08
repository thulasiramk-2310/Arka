mod config;
mod enforcers;

use std::time::Duration;
use tracing::{error, info};

fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .without_time()
        .init();

    info!("arkad starting");

    let cfg = config::Config::load();

    let enforcers: Vec<Box<dyn enforcers::Enforcer>> = vec![
        Box::new(enforcers::mac::MacEnforcer::new(&cfg)),
        Box::new(enforcers::dns::DnsEnforcer::new(&cfg)),
        Box::new(enforcers::hostname::HostnameEnforcer::new(&cfg)),
        Box::new(enforcers::ipv6::Ipv6Enforcer::new(&cfg)),
    ];

    // Initial enforcement pass
    for e in &enforcers {
        if let Err(err) = e.enforce() {
            error!(enforcer = e.name(), %err, "enforce failed");
        }
    }

    // Watch loop — re-verify and re-enforce on drift
    let interval = Duration::from_secs(cfg.check_interval_secs);
    loop {
        std::thread::sleep(interval);
        for e in &enforcers {
            match e.verify() {
                Ok(true) => {}
                Ok(false) => {
                    info!(enforcer = e.name(), "drift detected, re-enforcing");
                    if let Err(err) = e.enforce() {
                        error!(enforcer = e.name(), %err, "re-enforce failed");
                    }
                }
                Err(err) => error!(enforcer = e.name(), %err, "verify failed"),
            }
        }
    }
}
