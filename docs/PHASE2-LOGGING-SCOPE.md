# Phase 2 — Real per-attempt / per-connection logging (scoping note)

Status: **NOT STARTED. Do not build until the ethos question below is settled.**

Phase 1 (shipped as "Privacy Activity" / enforcement-made-visible) surfaces
arkad's enforcement *state* and *notable events*. It deliberately does **not**
log individual sandbox access attempts or individual outbound connections,
because those signals are (a) not observable without new instrumentation and
(b) in direct tension with what ArkaOS is for. This note records why, so a
future contributor inherits the constraint instead of rediscovering it.

## The two deferred signals

### A. Per-attempt sandbox blocks
Example we were asked for: *"Firefox tried to read /etc/machine-id — blocked
by sandbox."*

Not observable today. bubblewrap hides machine-id by **mount namespace** — the
file is simply absent inside the sandbox, so Firefox gets `ENOENT` internally
and **no block event is produced**. There is nothing to log. Phase 1 correctly
asserts the sandbox *guarantee* as a state ("Sandboxed · no home, no
machine-id") rather than faking an attempt that never surfaced.

To make real per-attempt blocks observable would require one of:
- seccomp with `SCMP_ACT_LOG` on the bwrap filter (logs *syscalls*, not paths —
  too coarse to name machine-id);
- auditd rules keyed to the sandbox uid/cgroup (logs `openat` attempts with
  paths — real, but see the ethos constraint below);
- eBPF tracing `openat` from the Firefox pid/cgroup.

Each is genuine work and must be its own verified effort, not a copy string.

### B. Per-app outbound connection log
Example: *"Firefox → quad9.net (DNS-over-TLS)."* as a per-query / per-connection
feed.

Technically doable (netlink `sock_diag` polling, or eBPF, mapping socket→pid→app
and IP→name). Phase 1 instead asserts the *state* ("DNS encrypted · Quad9 ·
DoT") from systemd-resolved, which is honest and needs no snooping.

## THE ETHOS CONSTRAINT (hard, not a nice-to-have)

**A per-domain / per-connection log is itself a surveillance layer inside a
privacy OS.** Recording every domain the user visits, or every file the browser
touches, creates exactly the persistent behavioral record ArkaOS exists to
prevent — and it becomes a target for compromise, subpoena, or drift into
"telemetry." The irony is the point: the feature that would most vividly *show*
privacy working is also the one most capable of *breaking* it.

Therefore, before any phase-2 logging is built, these must be answered and
written down — not assumed:
1. **Retention.** Default must be ephemeral (in-memory ring, wiped on close),
   never on-disk by default. Any persistence is explicit, opt-in, time-boxed.
2. **Granularity.** Prefer aggregate/state ("using encrypted DNS") over
   per-event ("→ example.com"). Name a domain only transiently, if at all.
3. **Consent + visibility.** The user must know logging is on, be able to turn
   it off, and it must never be silently enabled.
4. **Off by default.** The privacy-preserving default is *no* per-event log.
   The log is a diagnostic the user opts into, not a standing record.
5. **Honesty audit.** Every string still passes the standing pre-commit audit —
   claim nothing the mechanism can't prove.

If these can't all be satisfied, the honest answer is to **not build the
per-event log** and keep surfacing state only. Warning fatigue (users click
through ~70% of over-frequent security prompts) reinforces the same conclusion:
rare, aggregate, positive-state signals beat a firehose.

Cross-ref: honesty-over-marketing is a core project value; this note is its
application to observability.
