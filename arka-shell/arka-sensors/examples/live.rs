//! Print the captures in a `pw-dump` read from standard input, resolving
//! programs through /proc like the real overlay does.
//!   pw-dump | cargo run -p arka-sensors --example live
use arka_sensors::{describe, find_uses, Kind};
use std::io::Read;

fn main() {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).expect("read pw-dump output");
    let dump: serde_json::Value = serde_json::from_str(&raw).expect("pw-dump JSON");
    let exe = |pid: u32| std::fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.to_string_lossy().into_owned());
    let uses = find_uses(&dump, &exe);
    if uses.is_empty() {
        println!("nothing is capturing");
    }
    for kind in [Kind::Microphone, Kind::Camera, Kind::Screen] {
        if uses.iter().any(|u| u.kind == kind) {
            println!("{}", describe(kind, &uses));
        }
    }
    for u in &uses {
        println!("  detail: {:?}", u);
    }
}
