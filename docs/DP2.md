# DP2 — Developer Preview 2 (planning)

DP1 is frozen as a historical milestone (tag `v0.1.0-dp1`). DP2 is the first
release that **pays down architectural debt** left by the Hyprland → Plasma
pivot, while continuing to daily-drive ArkaOS and populate this file from real
usage.

DP2 is not primarily about new features. It is about *abstraction* — removing
the last places where the desktop applications assume a specific window
manager, so that ArkaOS can one day replace Plasma/KWin (with ArkaWM) without
rewriting the apps.

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

- **Vestigial shell crates.** `arka-bar`, `arka-dock`, `arka-launcher` were the
  custom Hyprland shell; under Plasma they are replaced by the panel,
  icontasks dock, and kickerdash launcher (see `arka-layout.js`) and are **not
  autostarted**. They still ship in the image and still contain `foot`/`thunar`
  references. Decision for DP2: **remove them** (drop the crates + their
  Containerfile `COPY`s) rather than port dead code. Confirm nothing references
  them first.
- Audit any other `hyprctl` / `swaymsg` / `foot` / `thunar` / `wofi` residue
  and route real needs through the appropriate abstraction or KDE equivalent.

## Screenshot grid

Real captures only — no mockups. Target set: Desktop · Launcher · Dashboard ·
Capsule · Settings · Welcome/Wizard · Lock Screen · Wallpaper · Privacy Report.
DP1 shipped with Desktop + Wizard; the rest land as DP2 is daily-driven.

## Later (not DP2)

- Real-hardware support, live ISO, thin installer (the DP2→Beta arc).
- ArkaWM as a `WindowService` backend — the payoff of this DP2 abstraction.
