# arka-pulse

The ArkaOS **reliability engine** — the counterpart to `arkad`:

```
arkad       → privacy      → protect the person
arka-pulse  → reliability  → protect the machine
```

**Status: experimental foundation.** This crate implements only the first two
stages of the loop in [`docs/RELIABILITY-ARKA-PULSE.md`](../docs/RELIABILITY-ARKA-PULSE.md):

```
MONITOR  ──▶  DETECT      ← implemented here (read-only, deterministic)
PREDICT · EXPLAIN · RECOVER · VERIFY   ← designed, NOT implemented
```

It reads `/proc` and `/sys`, applies deterministic threshold rules, and reports
findings. It has **no AI**, takes **no recovery action**, and writes **nothing**
to the system. It is **not wired into the OS image** — it is built and run
standalone while the design is proven against real behaviour.

## Design commitments (held from day one)

- **Read-only observation.** Every telemetry reader only reads `/proc` / `/sys`.
  Nothing opens a writable handle, spawns a process, or touches the network.
- **Deterministic core.** Same system state → same findings. Every finding
  carries the real numbers that triggered it (fact, never inference).
- **Service interface, not a concrete dependency.** Consumers depend on the
  `ReliabilityService` trait, never on the engine — the same swap-friendly
  discipline `WindowService` gives the desktop.
- **No AI in the safety path.** If an explanation layer is ever added, it stays
  optional and cannot reach the system; recovery, if it ever ships, ships
  disabled and dry-run behind an explicit policy gate. See the doc.

## Layout

```
src/
  model.rs        Severity, Finding — pure data
  monitor/        MONITOR — read-only telemetry
    cpu.rs        load averages + utilisation (/proc/loadavg, /proc/stat)
    memory.rs     RAM + swap (/proc/meminfo)
    psi.rs        pressure stall info (/proc/pressure/*)
    thermal.rs    zone temperatures (/sys/class/thermal)
  detect/         DETECT — deterministic threshold rules
  service.rs      ReliabilityService trait + PulseEngine (its implementation)
  main.rs         thin driver over the service
```

## Run

```
cargo run -- --once            # one reading, then exit
cargo run -- --interval 5      # continuous, every 5s
```

Example:

```
[08:22:28] health=OK   load(1/5/15)=0.86/0.60/1.02 mem=39% cpu=17%
    OK   system: System healthy — no action needed (mem 39% · load 0.86 · 46°C)
```

No dependencies — std only, so the deterministic core stays zero-footprint and
trivially auditable, matching `arkad`.
