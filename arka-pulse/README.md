# arka-pulse

The ArkaOS **reliability engine** — the counterpart to `arkad`:

```
arkad       → privacy      → protect the person
arka-pulse  → reliability  → protect the machine
```

**Status: experimental foundation.** This crate implements the first four
stages of the loop in [`docs/RELIABILITY-ARKA-PULSE.md`](../docs/RELIABILITY-ARKA-PULSE.md):

```
MONITOR ──▶ DETECT ──▶ PREDICT ──▶ EXPLAIN   ← implemented (deterministic)
RECOVER · VERIFY                             ← designed, NOT implemented
```

It reads `/proc` and `/sys`, applies deterministic threshold rules, projects
recent trends toward those thresholds, and produces a plain-language account. It
has **no AI running**, takes **no recovery action**, and writes **nothing** to
the system. It is **not wired into the OS image** — it is built and run
standalone while the design is proven against real behaviour.

- **PREDICT** is ordinary least-squares over a time window, gated hard against
  false alarms (needs enough consistent history, a real upward slope, a good
  fit, and a crossing inside a 30-min horizon — otherwise it stays silent). Its
  probability is an honest *heuristic*, not a calibrated figure, and the output
  says so ("est. probability / heuristic confidence").
- **EXPLAIN** runs a deterministic `FallbackExplainer` today, and already
  carries the *model-is-untrusted* boundary in code: a **sanitiser** (redacts
  secrets before any context could reach a model) and a **validator** (rejects
  malformed or command-like model output and maps `intent` to a fixed action
  registry). A local-LLM backend is the pluggable seam behind the `Explainer`
  trait — **not implemented**; if added, its output must pass the validator, and
  any failure degrades to the fallback.

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
  predict/        PREDICT — least-squares trend projection (history + gates)
  explain/        EXPLAIN — deterministic account + the untrusted-model boundary
    sanitize.rs   redact secrets/tokens/home-paths before any model sees context
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
