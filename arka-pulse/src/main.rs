//! arka-pulse — the ArkaOS reliability engine (experimental foundation).
//!
//! Status: this binary implements the first four stages of the loop
//! described in `docs/RELIABILITY-ARKA-PULSE.md`:
//!
//!     MONITOR ──▶ DETECT ──▶ PREDICT ──▶ EXPLAIN   (implemented, deterministic)
//!     RECOVER · VERIFY                             (designed, NOT implemented)
//!
//! EXPLAIN runs deterministically (a fallback explainer) and already carries
//! the "model is untrusted" gate — a sanitiser and a validator — ready for a
//! future local-LLM backend that is NOT implemented. So no model runs today.
//!
//! It reads `/proc` and `/sys`, works through the [`ReliabilityService`]
//! interface, and prints findings, predictions, and an explanation. It has
//! **no** AI running,
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
    for p in &s.predictions {
        println!(
            "    PRED {}: {} — est. probability ~{:.0}%, heuristic confidence ~{:.0}%",
            p.domain,
            p.summary,
            p.probability * 100.0,
            p.confidence * 100.0
        );
        println!("         {}", p.evidence);
    }
    if let Some(ex) = &s.explanation {
        println!("    EXPLAIN [{}] {}", ex.source.label(), ex.diagnosis);
        println!("            impact: {}", ex.impact);
        println!(
            "            proposed intent: {} (risk: {}) — NOT executed (no RECOVER stage)",
            ex.intent.id(),
            ex.intent.risk()
        );
    }
}

fn main() {
    let args = parse_args();

    eprintln!(
        "arka-pulse 0.0.1 — MONITOR + DETECT + PREDICT + EXPLAIN (deterministic). Read-only, dry-run.\n\
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
