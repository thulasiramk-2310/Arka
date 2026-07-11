# Field notes — living with ArkaOS

How we evaluate ArkaOS during the daily-driving period. **Not a bug tracker.**
Raw defects and friction go to `docs/DP2.md`; this file captures the *experience*,
because that's what shapes a desktop OS. Four lenses, asked honestly:

1. **What surprised me** — the moments I didn't expect, good or bad.
2. **What felt better** — than Windows or other Linux desktops.
3. **What still felt awkward** — friction that a real user would feel.
4. **What I stopped noticing** — because it "just worked." (The highest praise.)

Add an entry per meaningful session of use. Date it. Say who observed
(the person living in it, or the assistant during a review). Keep it honest —
label first-contact impressions differently from lived, habitual use, because
they measure different things.

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

<!-- Next entry template:

## YYYY-MM-DD — <context> (<who>)

**Surprised**
-
**Better than other desktops**
-
**Awkward**
-
**Stopped noticing ("just worked")**
-

-->
