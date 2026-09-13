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

## 2026-09-04 — dock too small (Ram, daily use)

**Awkward or frustrating**
- The floating bottom dock felt too small to use comfortably — icons undersized,
  hit targets fiddly. This is a *sizing* miss, not a paradigm one: the dock
  identity (floating, centered, icons-only — the mac-flavoured shape) is right;
  it was just too thin.

**Decision / fix applied (DP2)**
- Kept the dock paradigm — did **not** switch to a Windows-style taskbar (that
  would collapse the deliberate two-surface layout: slim top bar for status +
  floating dock for apps) and did **not** reach for Latte-dock magnification
  (unmaintained on Plasma 6 — a maintain-forever trap).
- Bumped the dock in `arka-layout.js` from `height = 48` → `64`; Plasma's
  `icontasks` scales icon size with panel thickness, so this gives larger icons
  and easier hit targets while staying dock-shaped (floating, centered, `fit`).
- Not yet proven in a booted VM — verify on real desktop before calling it done;
  72px is the next step if 64 still feels tight on hardware.

---

## 2026-09-13 — FIRST BARE-METAL BOOT (Ram + assistant, real hardware)

> ArkaOS booted on a **real laptop** for the first time ever — not a VM. Machine:
> **Nokia PureBook, Intel Core i5-10210U, Intel UHD graphics, 8 GB RAM**, AMI
> Aptio UEFI. Booted **externally from a USB stick** (BIB raw image `dd`'d to a
> SanDisk 3.2Gen1); the internal Windows disk was never touched. This clears the
> DP2 roadmap's "real hardware" line.

**Surprised / better than expected**
- It just *worked* on real metal, first try: GRUB → kernel → systemd → Plymouth →
  firstboot wizard → KDE Plasma desktop, at **full native resolution** (the
  low-res boot console corrected itself once i915 + Plasma took over — cosmetic).
- **arka-pulse read a real thermal sensor — 51 °C** (plus real CPU/mem) in the
  Privacy Dashboard's System Health card. First genuine-hardware reading ever (the
  VM showed "—"). This is the stage-2 "real-hardware deployment" rung, and on a
  *second* machine — exactly what the "stability across machines" calibration
  question in RELIABILITY-ARKA-PULSE.md wants.

**"It just works" (bare metal)**
- Wi-Fi connects, touchpad works, brightness + volume keys work, suspend/resume
  works. arkad: PrivacyScore 100/100 (DoT/MAC/hostname/IPv6 all enforced).

**Awkward / bugs (real use surfaced them — batch-fix then reflash)**
1. **No audio in sandboxed Firefox.** Volume 100 %, system audio fine (Intel sink
   present), but the browser is silent. Cause: the bwrap wrapper `arkaos-firefox`
   threaded only the Wayland socket into the sandbox, not the audio socket, so
   Firefox couldn't reach PipeWire. **Fix staged** (bind `pipewire-0` + `pulse`
   under `/run/user/UID`); pending a rebuild+reflash to verify on hardware.
2. **Capsule (app store) install did nothing.** Root cause: the image ships **no
   flatpak remote**, so `flatpak install … flathub …` failed with "remote not
   found." **Fix staged** — Capsule now adds the Flathub remote at `--user` level
   (no root) and installs `--user` before running. Pending rebuild+reflash.
3. **App icons render monochrome / miss their real colours — OS-wide, not just
   Capsule.** ArkaOS sets `Icons=Arka`, but `arka-icons/` is sparse and inherits
   breeze-dark; where neither supplies a coloured app icon, KDE/GTK falls back to
   a flat `*-symbolic` glyph. Capsule's catalogue is the same root cause (it uses
   `chat-symbolic`, `dialog-password-symbolic`, … by name). **Not a quick fix —
   completing/curating the icon theme is a dedicated task** (bundle real app
   icons, or map Flathub app-ids → coloured icons). Logged for a focused pass.

**Display / UX polish pass (queued — do as ONE batch, scaling first)**
These are likely interrelated, not four separate bugs — resolve in order:
- **"Everything looks small" (GRUB, SDDM, general) — the keystone.** Probably a
  **display-scaling** issue (HiDPI panel at 100 %). *Blocked on data:* need the
  Nokia's Settings → Display **resolution + scale %** before fixing. Fixing this
  may dissolve several of the items below.
- **Apps don't fill when maximized** — Privacy Dashboard = `adw::Clamp
  { maximum_size: 720 }` (centred narrow column); other Arka apps need a per-window
  expand check. Re-evaluate *after* scaling is set.
- **Wallpaper flash** — KDE default shows ~5–10 s, then Arka. `arka-plasma-firstrun`
  applies the wallpaper *after* Plasma starts. Fix: pre-seed the Arka wallpaper into
  `/etc/skel` Plasma config so it's the first frame (keep firstrun as fallback).
- **SDDM login theme looks dated** — visual refresh (modernise the custom theme).
- **Containerfile note:** `arkaos-firefox` COPY sits *above* the KDE dnf layer, so
  editing the wrapper busts the multi-hour KDE cache. Move it below the KDE layer.

**Deployment gotcha (remember this)**
- `dd` of the BIB raw image onto a *larger* USB leaves a **"primary GPT corrupt /
  PMBR size mismatch"**; strict AMI firmware then won't enumerate the stick as
  bootable (no UEFI-USB entry appears). Fix: `sudo sgdisk -e /dev/sdX` (relocate
  the backup GPT to the physical end + rewrite a valid primary), then it boots.
  Worth making BIB/first-boot self-heal this, or documenting it in BUILDING.md.

**Would I miss it?** First real-hardware session, so too early — but seeing the
green ARKA desktop fill a real panel, with Wi-Fi and the privacy score live, is
the first time ArkaOS felt like an actual OS rather than a VM demo.

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
