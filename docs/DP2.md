# DP2 — post-DP1 work already landed on `main`

DP1 is frozen as a historical milestone (tag `v0.1.0-dp1`). This file records
the concrete work that has **already landed on `main` since DP1** and will ship
in the next image build — it is a record of what's queued, not a plan.

**DP2's actual scope is not decided here.** It is determined after a month of
daily driving, from real experience (`docs/FIELD-NOTES.md`). Candidate future
work — hardware, FDE, installer, the service-abstraction direction, and the rest
— lives in `docs/FUTURE-CONSIDERATIONS.md`, which is committed to nothing.

The theme of what has landed so far is *paying down Hyprland → Plasma debt*.

## Carried over from DP1 (fixed on `main`, ships in DP2)

- **Lock Screen action** in Capsule/Bar used `swaylock` (Hyprland-era, not in
  the KDE image) → dead button. Now `loginctl lock-session`, which triggers
  KDE's kscreenlocker. The lock *screen* itself was always KDE and always
  worked; only the shell's shortcut to it was broken.
- **Terminal / file-manager actions** across Capsule, Update, and Settings used
  `foot` / `thunar` (Hyprland-era). Now `konsole` / `dolphin`, which the
  `@kde-desktop-environment` group actually installs.

These are source-only fixes and do **not** change the tagged DP1 image; they
land in the next build.

## Window-management abstraction (the headline of DP2)

Capsule's "Running" tab previously called `hyprctl` directly — a hard coupling
to a compositor ArkaOS no longer ships. That coupling is gone.

```
    Capsule ─▶ WindowService ─▶ KWin        (today)
    Capsule ─▶ WindowService ─▶ KWin/ArkaWM (tomorrow)
```

- `arka-shell-common::window` defines a compositor-neutral `Window` type and a
  `WindowService` trait (`list` / `focus` / `close`). Apps speak only this.
- `window_service()` selects the backend at runtime. Today it returns the KWin
  backend; a future ArkaWM backend slots in here with no caller change.
- The KWin backend drives KWin's Scripting D-Bus interface via `dbus-send`
  (universally present, unlike the version-variable `qdbus`).

**Verification status:** the abstraction and the Capsule refactor compile and
are the correct architecture. The KWin backend's *runtime* behaviour —
especially `list()`, which reads window info back from the user journal — is
**to be verified live during the DP2 build cycle**. It is not yet claimed
working on a booted system. (Per project rule: compile success ≠ runtime
correctness.)

## Remaining Hyprland-era debt to clear

- **Vestigial shell crates — DONE (moved to `legacy/`).** `arka-bar`,
  `arka-dock`, `arka-launcher` were the custom Hyprland shell; under Plasma they
  are replaced by the panel, icontasks dock, and kickerdash launcher (see
  `arka-layout.js`) and were never autostarted. Confirmed no desktop-file,
  autostart, layout, or keybinding referenced them. They are now:
  - moved to `legacy/` (own Cargo workspace; see `legacy/README.md`),
  - removed from the active `arka-shell` workspace,
  - no longer built or `COPY`'d into the image, and
  - the now-unused `gtk4-layer-shell` toolchain was dropped from both the
    builder and the runtime package set.

  Source kept for a release or two rather than deleted, to preserve project
  history. Remove for good once confirmed truly unused. (The active `arka-shell`
  workspace now `cargo check`s cleanly on a host with no layer-shell.)
- Audit any other `hyprctl` / `swaymsg` / `foot` / `thunar` / `wofi` residue
  and route real needs through the appropriate abstraction or KDE equivalent.

## Capsule distribution model

Capsule is a **Flathub client, not a publisher.** It runs
`flatpak install flathub <app-id>` — pulling from Flathub's remote exactly like
KDE Discover or GNOME Software — and never hosts, redistributes, or re-signs
anyone's binaries. That role needs no per-developer agreement (an app store
does not need a vendor's sign-off to offer an install button for a public
Flathub app). Agreements/rights only become necessary if ArkaOS later (a) hosts
its own app repository and redistributes binaries, (b) bundles a proprietary app
into the image (that app's EULA applies), or (c) implies partnership/endorsement.

Two hygiene items: pull catalog metadata (names, icons, descriptions) from
Flathub's AppStream instead of hand-curating, so branding stays aligned and
self-updates; and let proprietary-app EULAs be accepted by the user at install
time, not by ArkaOS.

## Screenshot grid — done

Real captures only — no mockups. The full DP1 set was captured live (2026-07-11)
and archived under `assets/screenshots/dp1/`: desktop, lock screen, wizard,
launcher, Privacy Dashboard, Privacy Report, Capsule, Settings, Wi-Fi. Shown in
the README.

---

Forward-looking candidate work (hardware, FDE, installer, service abstractions,
ecosystem, accessibility, …) is intentionally **not** here — it lives in
`docs/FUTURE-CONSIDERATIONS.md`, committed to nothing, so this file stays a
factual record of landed work rather than a speculative roadmap.
