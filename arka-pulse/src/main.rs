//! arka-pulse — the ArkaOS reliability engine (experimental foundation).
//!
//! Status: this binary implements only the first two stages of the loop
//! described in `docs/RELIABILITY-ARKA-PULSE.md`:
//!
//!     MONITOR  ──▶  DETECT      (implemented — read-only, deterministic)
//!     PREDICT · EXPLAIN · RECOVER · VERIFY   (designed, NOT implemented)
//!
//! It reads `/proc` and `/sys`, applies deterministic threshold rules through
//! the [`ReliabilityService`] interface, and prints findings. It has **no** AI,
//! takes **no** recovery action, and writes **nothing** to the system.
//!
//! This crate is clean-room ArkaOS code; it is not wired into the OS image.
//!
//! Usage:
//!     arka-pulse [--once] [--interval SECONDS]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arka_pulse::service::{HealthSnapshot, PulseEngine, ReliabilityService};

struct Args {
    once: bool,
    interval: Duration,
}

fn parse_args() -> Args {
    let mut once = false;
    let mut interval = Duration::from_secs(10);
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--once" => once = true,
            "--interval" => {
                if let Some(v) = it.next().and_then(|s| s.parse::<u64>().ok()) {
                    interval = Duration::from_secs(v.max(1));
                }
            }
            "-h" | "--help" => {
                println!("arka-pulse [--once] [--interval SECONDS]");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    Args { once, interval }
}

/// UTC HH:MM:SS from the wall clock, without pulling in a date library.
fn clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn report(s: &HealthSnapshot) {
    let t = &s.telemetry;
    let util = match t.cpu_util {
        Some(u) => format!("{u:.0}%"),
        None => "--".to_string(),
    };
    println!(
        "[{}] health={:<4} load(1/5/15)={:.2}/{:.2}/{:.2} mem={:.0}% cpu={}",
        clock(),
        s.worst.label(),
        t.load1,
        t.load5,
        t.load15,
        t.memory.used_pct(),
        util
    );
    for f in &s.findings {
        println!(
            "    {:<4} {}: {} ({})",
            f.severity, f.domain, f.summary, f.evidence
        );
    }
}

fn main() {
    let args = parse_args();

    eprintln!(
        "arka-pulse 0.0.1 — MONITOR + DETECT only. Read-only, dry-run: makes no changes.\n\
         See docs/RELIABILITY-ARKA-PULSE.md for the full design.\n"
    );

    let mut engine = PulseEngine::new();

    // A short settle before the first read (so utilisation is meaningful),
    // then the requested cadence for subsequent reads.
    let mut gap = if args.once {
        Duration::from_millis(500)
    } else {
        args.interval
    };

    loop {
        std::thread::sleep(gap);
        match engine.health() {
            Ok(snapshot) => report(&snapshot),
            Err(e) => eprintln!("[{}] monitor error: {e}", clock()),
        }
        if args.once {
            break;
        }
        gap = args.interval;
    }
}
