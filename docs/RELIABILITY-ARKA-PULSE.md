# Reliability layer — arka-pulse

**Status: future direction. Committed to nothing.** This document describes a
candidate architecture for a system-reliability layer in ArkaOS. It is a
parking-lot design in the sense of [`FUTURE-CONSIDERATIONS.md`](FUTURE-CONSIDERATIONS.md):
its presence here is not a promise, and **nothing in it is to be integrated
during the DP1 feature freeze.** The trigger for revisiting it is real
daily-driving evidence (see [`FIELD-NOTES.md`](FIELD-NOTES.md)), not this page.

> arka-pulse is the ArkaOS name for the engine prototyped as **Kernelpulse** — a
> hackathon project (*"Predictive Fault Intelligence and Autonomous Recovery
> Framework for Linux"*). This doc adapts that architecture to ArkaOS; it does
> not adopt the hackathon implementation as-is. See *Honest status* below for
> what actually exists today versus what is only designed.

---

## Why this fits ArkaOS

ArkaOS today protects one thing: **the person** — `arkad` enforces privacy
invariants (MAC randomization, DoT, hostname, IPv6 temp addressing). arka-pulse
would protect the other thing: **the machine** — its health and reliability.

```
arkad       → privacy      → protect the person
arka-pulse  → reliability  → protect the machine
```

The product story that combination unlocks is not "your computer is private."
It is:

> *Your computer is private, and it watches its own health — locally, without
> handing your telemetry to anyone.*

That "locally" is the whole point, and it is why this belongs in **ArkaOS**
specifically rather than being a generic monitoring add-on. The Kernelpulse
design already runs all inference on-device (local `llama.cpp`) and sanitizes
telemetry before it ever reaches a model. A privacy-first OS is the natural home
for a reliability engine that refuses to phone home.

---

## The loop

The engine is a closed control loop. Each stage is deterministic except
**EXPLAIN**, which is the only place a model is involved — and its output is
treated as untrusted (see *The model is untrusted*).

```
        ┌──────────────────────────────────────────────┐
        │                                                │
        ▼                                                │
    MONITOR  ──▶  DETECT  ──▶  PREDICT  ──▶  EXPLAIN  ──▶ RECOVER ──▶ VERIFY
   read-only    rules +      failure-      local AI,     policy-      re-check
   telemetry    IsolationF.  domain        structured    gated        telemetry
   /proc /sys   anomalies    probability   JSON only     actions      resolved?
        ▲                                                                │
        └────────────────────────────────────────────────────────────────┘
                              back to MONITOR
```

| Stage | Question it answers | Backend module |
|-------|---------------------|----------------|
| MONITOR | *What is the system doing?* | `monitoring/` (cpu, memory, psi, disk, network, thermal, gpu, kernel, services, drivers, hardware) |
| DETECT | *What is abnormal?* | `detection/` — deterministic rule engine fused with `IsolationForest` (multivariate baseline deviation) |
| PREDICT | *Will the abnormal become a failure?* | `prediction/` — temporal features (slopes, moving averages) → probability + confidence + lead-time, per domain |
| EXPLAIN | *What does this mean, in words?* | `context/` (sanitize + relevance-filter) → `ai/` (local LLM → structured JSON) |
| RECOVER | *What safe action fixes it?* | `recovery/` planner → action registry → `policy/` gate → executor |
| VERIFY | *Did the action work?* | `recovery/verifier.py` — immediate re-evaluation → `RECOVERY_SUCCESS` / `RECOVERY_FAILED` |

Prediction targets are scoped to concrete domains, each with its own evidence:
`THERMAL_INSTABILITY` (CPU moving averages, acceleration slopes),
`RESOURCE_INSTABILITY` (memory/swap slopes, CPU PSI),
`DRIVER_INSTABILITY` (device reset frequency, driver-error acceleration),
`NETWORK_DRIVER_INSTABILITY` (RX/TX error rates, packet-drop acceleration,
disconnect frequency).

---

## The model is untrusted

This is the single most important principle to carry over, and the one that
makes arka-pulse compatible with the ArkaOS threat model. **The AI never touches
the system.** It observes, explains, and recommends — nothing else. Every path
from a model's output to an actual system change passes through deterministic
gates that can reject it.

```
  LLM output (fuzzy text)
        │
        ▼
  JSONValidator ......... schema-valid? dangerous strings (e.g. `rm -rf`)? → REJECT
        │
        ▼
  Recovery Planner ...... map to a hardcoded Action Registry ID, else → INVALID_ACTION
        │
        ▼
  Policy Engine ......... by RiskLevel:
        │                   LOW      → allowed automatically
        │                   MED/HIGH → blocked, requires human approval
        │                   CRITICAL → denied outright (e.g. reboot)
        ▼
  Executor .............. predefined subprocess ARRAY — never shell=True,
                          never interpolates LLM strings
```

Hard boundaries, stated as invariants:

- **Read-only AI.** The LLM cannot run shell commands, restart services, or
  modify files. It emits a recommendation string; that is all.
- **Action Registry only.** Only commands predefined in source can ever run.
- **No shell.** Subprocess calls never use `shell=True` and never interpolate
  model output.
- **Fact vs inference is explicit.** *Observed* (real telemetry) and *Predicted*
  (deterministic probabilities) are separated from *AI interpretation*. The AI
  does not predict the failure and cannot override the deterministic probability.
- **Local + sanitized.** Inference runs on-device; a `DataSanitizer` redacts
  passwords, tokens, keys, and private strings before any context reaches a model.
- **Graceful degradation.** If the model is unavailable, times out, or fails
  validation, the system falls back to a deterministic `FallbackExplanation`
  built from the metrics — operators still get an actionable alert.
- **Dry-run by default.** Recovery ships `enabled: false, dry_run: true`. It logs
  intent and changes nothing until a user explicitly reconfigures it.

The framing to hold onto: this is **AI-assisted, policy-governed reliability** —
never "AI controlling the OS." The deterministic layer is always in charge.

---

## Where it sits in ArkaOS

arka-pulse is a *backend*. ArkaOS apps would never call it directly — they call
a `ReliabilityService` interface, exactly as UI talks to `WindowService` today
(see the service-interface discipline in
[`FUTURE-CONSIDERATIONS.md`](FUTURE-CONSIDERATIONS.md)).

```
ArkaOS
│
├── PrivacyService     → arkad          (exists)
├── WindowService      → KWin           (exists)
├── PowerService       → PowerDevil
├── NetworkService     → NetworkManager
├── UpdateService      → bootc
│
└── ReliabilityService → arka-pulse     (candidate)
```

The UI does not know or care whether reliability is implemented by today's
arka-pulse engine or something entirely different later. That is the point of the
interface — the same LEGO-swap property that keeps the "bring ArkaWM" door open.

### UX: one surface, two depths

The reliability layer should be invisible when things are fine and legible when
they are not. Same data, two audiences.

Normal user — a single verdict, nothing technical:

```
🟢 System Health
Your computer is healthy. No action needed.
```

Advanced mode — the full evidence chain:

```
Reliability
  CPU      Normal      Memory   Normal      Thermal  Normal
  Drivers  Normal      Network  Normal
  Predicted instability   None
  Last recovery           Network service restart · verified 14:32
```

And when something is actually predicted, the recommendation carries its risk and
asks before acting — never a silent mutation:

```
⚠ Network may become unstable
ArkaOS detected increasing network errors.
Recommended: restart network service     Risk: Low
[ Fix Automatically ]   [ Review ]
        │
        ▼
✓ Fixed — connection restored. ArkaOS verified the recovery.
```

---

## Honest status

Being accurate about maturity matters here as much as anywhere in ArkaOS.

**What exists now** (in `arka-pulse/`): a **clean-room Rust** crate — not the
Python Kernelpulse prototype — implementing the *entire* loop end-to-end,
**read-only and dry-run**, zero dependencies, 29 unit tests:

- `MONITOR → DETECT → PREDICT` are deterministic and read `/proc` + `/sys` only.
- `PREDICT` is least-squares trend projection, hard-gated against false alarms
  (history, slope, fit, horizon), reported as a labelled *heuristic* — never a
  calibrated figure.
- `EXPLAIN` is a **deterministic fallback**; no model runs. The
  model-is-untrusted boundary already exists in code — a sanitiser and a
  validator that rejects malformed/command-like output and maps `intent` to a
  fixed action registry.
- `RECOVER` is **dry-run only**: registry → policy engine → an executor that
  *logs* a fixed argv and **never spawns a process**. No real executor exists;
  it ships disabled.
- `VERIFY` re-samples and refuses to claim recovery when nothing was applied.

**What is NOT proven:** prediction *calibration* on real hardware. Synthetic
correctness holds; real-world precision, false-positive rate, and lead-time are
**unmeasured** — a VM answers none of the hardware questions, exactly the DP1
lesson.

So the honest one-line summary: **the loop and its safety boundaries are
implemented and demonstrated in dry-run; the predictive and self-healing claims
are not yet earned on real hardware.**

---

## The evidence ladder

The engine is paused here deliberately. Each stage must *earn* the next — the
next uncertainty is not "can we build it?" (demonstrated) but "does prediction
hold on real behaviour well enough to justify an LLM and privileged recovery?"

```
1  Synthetic correctness        ✅ done (dry-run loop + tests)
2  Read-only real-hardware deployment  ✅ runs on real hardware (see below)
3  Prediction calibration       ← the gate that earns everything after it
4  Local explanation (LLM)
5  Policy-gated recovery (real executor)
6  Verified autonomous recovery
```

**Stage 2 — real-hardware run (2026-09-11, ASUS ROG Zephyrus G14 GA403UV, Arch,
kernel 7.1.8).** The release binary ran on a real machine for the first time (not
a VM): MONITOR read genuine `/proc`+`/sys` telemetry — CPU util, load, memory, and
real thermals (39–41 °C) — across multiple samples with a coherent `OK` verdict,
and the `--demo` synthetic incident drove the full EXPLAIN → RECOVER → VERIFY chain
with the dry-run boundary holding (logged `sysctl vm.drop_caches=1`, executed
nothing, VERIFY reported NOT-APPLIED). What this rung proves is only that the
engine *reads real hardware correctly and stays inert* — it does **not** touch
stage 3. Calibration needs sustained observation across real load and real faults,
which a single healthy idle machine cannot supply.

**Do not call a prediction a success merely because it produced a prediction.**
Before EXPLAIN gains an LLM or RECOVER gains a real executor, measure prediction
quality on real telemetry:

- **precision** — how often predicted instability actually develops;
- **false-positive rate** — the metric that decides whether users trust it;
- **lead time** — how far ahead, per warning;
- **missed failures** — what it failed to see coming;
- **per-class performance** — memory / swap / thermal separately;
- **stability across machines** — does it generalise, or overfit one box.

Future telemetry sources that make prediction real (not in scope now): SMART
attributes, EDAC (memory ECC), MCE (machine-check exceptions), and eBPF probes.

Only stage 3 passing justifies stage 4; only stage 4 justifies stage 5. Filling
the architecture's LLM/executor slots ahead of that evidence is precisely the
mistake this ladder exists to prevent.

---

## Open questions before any integration

These are the things to resolve *before* arka-pulse becomes more than a doc.
None is blocking today; all are load-bearing later.

- **Runtime weight vs. the immutable image.** `arkad` is a static musl Rust
  binary. Kernelpulse is Python 3.14 + scikit-learn + a local LLM. Bundling a
  Python ML stack and a GGUF model into a bootc image is a real size and
  maintenance cost. Options: keep it Python and accept the weight; rewrite the
  deterministic core in Rust to match `arkad` and keep only the LLM external;
  ship the model as a separately-pulled layer. Undecided.
- **Privilege model.** Some telemetry needs elevated access. What exactly runs as
  root, and how is that surface kept minimal — same discipline as `arkad`.
- **Local model distribution.** A useful GGUF is hundreds of MB to gigabytes.
  How does it fit an immutable, verifiable image without bloating every update?
- **Does the reliability core even need the LLM?** The deterministic detector,
  predictor, policy gate, and action registry are the safety-critical parts and
  work without a model. The LLM only makes explanations human-friendly. It should
  be possible to ship reliability with the AI layer fully optional.
- **Ownership.** Kernelpulse is a separate repo under a different author. Any
  integration needs a clear licensing and maintenance boundary.

---

## Guardrails

- **Not "AI controlling the OS."** If a future version blurs the deterministic
  gates — lets model output reach execution without the validator, planner,
  policy, and action-registry chain — that is a regression, not a feature.
- **Reliability is not telemetry.** Nothing about this layer weakens the privacy
  promise. Local-only, sanitized, no analytics — the same bar `arkad` holds.
- **Earn the claims.** Do not ship "predicts failures" or "self-heals" as product
  language until real hardware and real use back it up. Honesty over marketing.

---

## When

Not now. After the month of real-world DP1 use finishes, `FIELD-NOTES.md` and
actual hardware findings decide whether reliability is a problem worth solving in
DP2 at all — and if so, whether arka-pulse is the right shape for it.
</content>
</invoke>
