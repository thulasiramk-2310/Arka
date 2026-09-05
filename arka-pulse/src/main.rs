//! arka-pulse — the ArkaOS reliability engine (experimental foundation).
//!
//! The full loop from `docs/RELIABILITY-ARKA-PULSE.md` now runs end-to-end,
//! but the acting stage is inert:
//!
//!     MONITOR ─▶ DETECT ─▶ PREDICT ─▶ EXPLAIN ─▶ RECOVER ─▶ VERIFY
//!     └──────── read-only, deterministic ───────┘  dry-run    re-sample
//!
//! - MONITOR/DETECT/PREDICT read `/proc` and `/sys` and are deterministic.
//! - EXPLAIN is a deterministic fallback behind the model-is-untrusted gate;
//!   no model runs.
//! - RECOVER is **dry-run only**: it maps an intent to a fixed argv through the
//!   action registry and policy engine, then *logs* what it would do. There is
//!   no real executor — nothing is ever spawned. It ships disabled by default.
//! - VERIFY re-samples to confirm outcomes; since nothing is ever applied, it
//!   reports NOT-APPLIED rather than claiming a recovery.
//!
//! This crate is clean-room ArkaOS code; it is not wired into the OS image.
//!
//! Usage:
//!     arka-pulse [--once] [--interval SECONDS] [--recover-dryrun] [--demo]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arka_pulse::explain::{Explainer, FallbackExplainer, Incident, Risk};
use arka_pulse::model::{Finding, Severity};
use arka_pulse::recover::{self, ExecOutcome, RecoveryConfig, RecoveryReport};
use arka_pulse::service::{HealthSnapshot, PulseEngine, ReliabilityService};
use arka_pulse::verify::{self, VerifyOutcome};

struct Args {
    once: bool,
    interval: Duration,
    recover: bool,
    demo: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        once: false,
        interval: Duration::from_secs(10),
        recover: false,
        demo: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--once" => a.once = true,
            "--recover-dryrun" => a.recover = true,
            "--demo" => a.demo = true,
            "--interval" => {
                if let Some(v) = it.next().and_then(|s| s.parse::<u64>().ok()) {
                    a.interval = Duration::from_secs(v.max(1));
                }
            }
            "-h" | "--help" => {
                println!("arka-pulse [--once] [--interval SECONDS] [--recover-dryrun] [--demo]");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    a
}

/// UTC HH:MM:SS from the wall clock, without pulling in a date library.
fn clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    // UTC — std has no timezone support and arka-pulse stays zero-dependency,
    // so the label is explicit rather than silently showing UTC as if local.
    format!("{:02}:{:02}:{:02} UTC", s / 3600, (s % 3600) / 60, s % 60)
}

fn print_findings_and_predictions(s: &HealthSnapshot) {
    for f in &s.findings {
        println!("    {:<4} {}: {} ({})", f.severity, f.domain, f.summary, f.evidence);
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
            "            proposed intent: {} (risk: {})",
            ex.intent.id(),
            ex.intent.risk()
        );
    }
}

fn print_recovery(rep: &RecoveryReport, worst: Severity) {
    match &rep.action {
        None => println!(
            "    RECOVER intent={} decision={} (no registered action)",
            rep.intent.id(),
            rep.decision.label()
        ),
        Some(a) => {
            println!(
                "    RECOVER intent={} action=\"{}\" risk={} decision={}",
                rep.intent.id(),
                a.description,
                a.risk,
                rep.decision.label()
            );
            match &rep.exec {
                Some(ExecOutcome::DryRun { would_run }) => {
                    println!("            would run (dry-run, NOT executed): {would_run}")
                }
                Some(ExecOutcome::NotImplemented) => {
                    println!("            real executor not implemented — refused")
                }
                Some(ExecOutcome::Applied) => println!("            applied"),
                None => println!("            awaiting human approval — nothing run"),
            }
        }
    }

    // Nothing is applied in dry-run, so VERIFY reports NOT-APPLIED without even
    // re-sampling (the closure is never called when `applied` is false).
    let outcome = verify::verify(worst, rep.applied(), || Ok(worst));
    let note = match outcome {
        VerifyOutcome::NotApplied => " (dry-run: nothing applied, so recovery is not claimed)",
        _ => "",
    };
    println!("    VERIFY  {}{}", outcome, note);
}

/// Run the whole loop once over a clearly-labelled *synthetic* incident, so the
/// EXPLAIN → RECOVER → VERIFY chain is visible even on a healthy machine.
fn run_demo() {
    eprintln!("[DEMO] synthetic incident — NOT real telemetry.\n");

    let findings = vec![Finding::new(
        "memory",
        Severity::Critical,
        "Memory almost exhausted — the OOM killer is imminent",
        "97% used (synthetic)",
    )];
    let incident = Incident {
        findings: &findings,
        predictions: &[],
    };
    let ex = FallbackExplainer.explain(&incident);

    println!("[DEMO] health=CRIT (synthetic)");
    for f in &findings {
        println!("    {:<4} {}: {} ({})", f.severity, f.domain, f.summary, f.evidence);
    }
    println!("    EXPLAIN [{}] {}", ex.source.label(), ex.diagnosis);
    println!("            impact: {}", ex.impact);
    println!(
        "            proposed intent: {} (risk: {})",
        ex.intent.id(),
        ex.intent.risk()
    );

    // Recovery explicitly enabled + dry-run, to show the auto-approve path.
    let cfg = RecoveryConfig {
        enabled: true,
        dry_run: true,
        auto_max_risk: Risk::Low,
    };
    let rep = recover::plan(&ex, &cfg);
    print_recovery(&rep, Severity::Critical);

    eprintln!("\n[DEMO] end — nothing above touched the system.");
}

fn main() {
    let args = parse_args();

    eprintln!(
        "arka-pulse 0.0.1 — MONITOR+DETECT+PREDICT+EXPLAIN+RECOVER (dry-run). Read-only.\n\
         See docs/RELIABILITY-ARKA-PULSE.md for the full design.\n"
    );

    if args.demo {
        run_demo();
        return;
    }

    let recovery_cfg = if args.recover {
        RecoveryConfig {
            enabled: true,
            dry_run: true,
            auto_max_risk: Risk::Low,
        }
    } else {
        RecoveryConfig::default()
    };

    let mut engine = PulseEngine::new();

    let mut gap = if args.once {
        Duration::from_millis(500)
    } else {
        args.interval
    };

    loop {
        std::thread::sleep(gap);
        match engine.health() {
            Ok(snapshot) => {
                println!(
                    "[{}] health={:<4} load(1/5/15)={:.2}/{:.2}/{:.2} mem={:.0}% cpu={}",
                    clock(),
                    snapshot.worst.label(),
                    snapshot.telemetry.load1,
                    snapshot.telemetry.load5,
                    snapshot.telemetry.load15,
                    snapshot.telemetry.memory.used_pct(),
                    match snapshot.telemetry.cpu_util {
                        Some(u) => format!("{u:.0}%"),
                        None => "--".to_string(),
                    }
                );
                print_findings_and_predictions(&snapshot);
                if snapshot.explanation.is_some() {
                    let rep = recover::plan(snapshot.explanation.as_ref().unwrap(), &recovery_cfg);
                    print_recovery(&rep, snapshot.worst);
                }
            }
            Err(e) => eprintln!("[{}] monitor error: {e}", clock()),
        }
        if args.once {
            break;
        }
        gap = args.interval;
    }
}
