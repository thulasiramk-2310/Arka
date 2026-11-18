//! arka-install: the ArkaOS installer backend.
//!
//!   arka-install probe                      list the disks and what is on them (JSON)
//!   arka-install plan --disk D --mode M     plan an install with systemd-repart's dry run (JSON)
//!   arka-install apply --plan FILE          carry out a reviewed plan (passphrase on stdin)
//!
//! probe and plan write nothing to any disk. apply writes only after checking that the disk is
//! exactly as it was when the plan was made. Exit codes: 0 done, 1 error, 2 usage, 3 refused.

mod apply;
mod disk;
mod plan;
mod run;

use std::process::ExitCode;

const DEFAULT_TARGET_IMGREF: &str = "ghcr.io/thulasiramk-2310/arka:latest";

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  arka-install probe\n  arka-install plan --disk /dev/X --mode free|empty|erase --image IMAGE [--target-imgref REF] [--no-encrypt] [--keymap us] [--out FILE]\n  arka-install apply --plan FILE   (reads the passphrase from stdin when the plan encrypts)"
    );
    ExitCode::from(2)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn fail(code: u8, msg: &str) -> ExitCode {
    println!("{}", serde_json::json!({"event": if code == 3 { "refused" } else { "error" }, "message": msg}));
    run::log(&format!("{}: {msg}", if code == 3 { "refused" } else { "error" }));
    ExitCode::from(code)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { return usage() };
    if !run::is_root() {
        return fail(1, "arka-install must run as root (it reads partition tables and EFI partitions).");
    }
    let _ = std::fs::create_dir_all(plan::PLAN_DIR);
    match cmd.as_str() {
        "probe" => match disk::probe() {
            Ok(d) => {
                println!("{}", serde_json::to_string_pretty(&d).unwrap_or_default());
                ExitCode::SUCCESS
            }
            Err(e) => fail(1, &e),
        },
        "plan" => {
            let (Some(d), Some(m), Some(image)) = (flag(&args, "--disk"), flag(&args, "--mode"), flag(&args, "--image")) else {
                return usage();
            };
            let Some(mode) = plan::Mode::parse(&m) else { return usage() };
            let req = plan::Request {
                disk: d,
                mode,
                encrypt: !args.iter().any(|a| a == "--no-encrypt"),
                keymap: flag(&args, "--keymap").unwrap_or_else(|| "us".into()),
                image,
                target_imgref: flag(&args, "--target-imgref").unwrap_or_else(|| DEFAULT_TARGET_IMGREF.into()),
            };
            match plan::make(&req) {
                Ok(p) => {
                    let json = serde_json::to_string_pretty(&p).unwrap_or_default();
                    let out = flag(&args, "--out").unwrap_or_else(|| format!("{}/plan.json", plan::PLAN_DIR));
                    if let Err(e) = std::fs::write(&out, &json) {
                        return fail(1, &format!("could not save the plan to {out}: {e}"));
                    }
                    println!("{json}");
                    ExitCode::SUCCESS
                }
                Err(plan::Refusal(why)) => fail(3, &why),
            }
        }
        "apply" => {
            let Some(path) = flag(&args, "--plan") else { return usage() };
            let p: plan::Plan = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
                Ok(p) => p,
                Err(e) => return fail(1, &format!("could not read the plan {path}: {e}")),
            };
            match apply::apply(&p) {
                Ok(o) => {
                    println!("{}", serde_json::json!({"event": "done", "result": o}));
                    ExitCode::SUCCESS
                }
                Err(e) => fail(1, &e),
            }
        }
        _ => usage(),
    }
}
