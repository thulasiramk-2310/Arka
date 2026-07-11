# Field notes — living with ArkaOS

> **First-contact ≠ lived-in.** First impressions measure *excitement*; living
> with something measures *quality*. They are different metrics — always label
> which one an entry is.

The **official evaluation method for every release** — not just DP1 — and
**not a bug tracker.** Repeated real-world friction feeds
`docs/FUTURE-CONSIDERATIONS.md`; this file captures the *experience*, because
that's what shapes a desktop OS.

**Four lenses**, asked honestly. Note only one is negative — that's what keeps
this from collapsing into a defect list:

1. **What surprised me?** — moments I didn't expect, good or bad.
2. **What was better than expected?** — vs Windows or other Linux desktops.
3. **What felt awkward or frustrating?** — friction a real user would feel.
4. **What disappeared into "it just works"?** — the highest praise, and the
   hardest thing in software to earn. It only becomes true *after* the novelty
   wears off — it can't be faked on day one. If, after three weeks, you've
   stopped noticing updates, launcher, Wi-Fi, dashboard, and privacy: you've won.

**The rule while daily-driving** — don't open the IDE at the first annoyance:

```
notice it → write it down → keep using the OS
```

If something annoys you five times, it's worth fixing; once and never again,
probably not. Close the IDE more than you open it — for this month you are not
the lead developer, you are ArkaOS's first real user.

Add an entry per meaningful day or session. Date it, and say who observed (the
person living in it, or the assistant during a review).

---

## 2026-07-11 — first contact (assistant, during DP1 verification)

> Scope: a single session driving the DP1 image (wizard, launcher, dashboard,
> capsule, settings, lock screen). First impressions — **not** lived daily use.

**Surprised**
- Privacy expressed as a *score* — "100 / 100 · Your Computer Is Yours." Not a
  panel of switches but a state of being. Novel framing.
- Timeline wording: "DNS-over-TLS active — searches hidden from your internet
  provider." Plain cause-and-effect over `resolved.conf` jargon.

**Better than other desktops**
- End-to-end visual coherence: Plymouth → SDDM → lock → desktop → apps all share
  one identity. Most distros fracture the moment you leave DE defaults.
- Onboarding leads with what protects you, not partitions.
- Honest UI: "Lock Screen — Coming soon" instead of a toggle that lies.

**Awkward**
- Capsule "Running" tab feels adrift inside an app-installer (plumbing fixed for
  DP2; the feature's *place* is the open question).
- Settings "Automatic Login" read OFF while the machine autologged in — a state
  desync to chase (wizard sets SDDM autologin; Settings may not reflect it).
- Wi-Fi empty state is bare — fine in a VM, but it's the first thing a real user
  meets.

**Stopped noticing ("just worked")**
- Autologin → desktop, no ceremony.
- `arkad` — the score simply *was* 100; enforcement was invisible.
- The first-boot wizard just *appearing* (the two-month black-screen fight). You
  stop noticing the fix — which is the point.

---

<!-- Daily entry template:

## Day N — YYYY-MM-DD (<who>)

**Today I used**
- (browser, editor, terminal, git, music, …)

**Surprised**
-
**Better than expected**
-
**Awkward or frustrating**
-
**Disappeared into "it just works"**
-
**Workaround I used**
-
**Would I miss this if I went back to another OS?**
- (a "yes" here names a genuine differentiator — e.g. "I'd miss Capsule" or
  "I'd miss the Privacy Dashboard")

-->
