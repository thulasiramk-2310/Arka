# ArkaOS Developer Preview 1

**Version 0.1.0-dp1**

Privacy-first • Immutable • Built on Linux • Designed for people

## What is ArkaOS?

ArkaOS is a privacy-first, immutable desktop operating system built on
Linux, designed to make secure computing simple, intuitive, and accessible
for everyone.

ArkaOS isn't a replacement for Linux — it's a better way to experience
Linux: it builds on Linux's proven foundation while removing unnecessary
complexity and making powerful security and privacy features available by
default. The OS ships as an
immutable image — updates are atomic, rollback is built in, and the root
filesystem is read-only at runtime. A system daemon (arkad) continuously
enforces privacy invariants (randomized MAC, encrypted DNS, generic hostname,
IPv6 privacy addresses) and shows you what it did, in plain language, on a
privacy dashboard. The browser runs in a sandbox that cannot see your files.

It looks and feels like one product: one palette, one typeface, one icon
language, one motion system — from the boot splash to the login screen to
every app.

## What's in Developer Preview 1

- Immutable bootc/composefs foundation on Fedora 42, KDE Plasma desktop
  with the ArkaOS shell layout (top bar, floating dock, full-screen launcher)
- arkad privacy daemon + Dashboard with privacy score, event timeline, and
  weekly privacy report
- Sandboxed Firefox, permission manager, encrypted DNS out of the box
- Complete brand identity: boot animation, login theme, icons, cursor,
  sounds, wallpaper, design system v1.0
- Everyday basics: WiFi, Bluetooth, power profiles, file previews,
  system-wide search, clipboard history

## What's known to be incomplete

- **Hardware support:** DP1 is validated in QEMU/KVM virtual machines only.
  Bare metal may work (it's Fedora underneath) but is untested and unsupported.
- Measured boot stops at PCR 10 (bootloader limitation, documented).
- GTK/Qt widget metrics don't match 1:1 yet (audit in progress).
- No installer ISO yet — DP1 ships as a qcow2 disk image.
- English only.
- **Capsule "Lock Screen" quick action is a no-op.** It still references the
  legacy Hyprland locker (`swaylock`), which isn't installed in the KDE image.
  The actual lock screen (KDE kscreenlocker, e.g. via Meta+L or the system
  tray) works normally. Fixed on `main` (`loginctl lock-session`); ships in DP2.
  Same root cause as a few other Capsule/Settings actions that referenced
  Hyprland-era tools (`foot`, `thunar`) — all fixed on `main` for DP2.

## Requirements

QEMU/KVM with UEFI (OVMF), 4 GB RAM minimum (6 GB recommended), 20 GB disk.

## Reporting issues

Open a GitHub issue with: the version (`cat /usr/lib/arkaos-release` or this
file's version), what you did, what you expected, what happened, and the
output of `systemctl --failed` and `journalctl -b -p err` if relevant.

## Building from source

See `docs/BUILDING.md`. Architecture rationale: `docs/ARCHITECTURE.md`.
