# Changelog

## v0.1.0-dp1 — Developer Preview 1 (2026-07-11)

### Added
- Immutable OS foundation: bootc image model, composefs read-only rootfs,
  atomic updates with rollback, TPM PCR 0–10 measurement.
- `arkad` privacy daemon: MAC randomization, encrypted DNS (DoT/Quad9),
  hostname normalization, IPv6 privacy addresses; enforce-on-boot +
  re-verify every 60 s; privacy event log + D-Bus interface + privacy score.
- Firefox bubblewrap sandbox: tmpfs home, /etc allowlist, no D-Bus,
  Downloads-only passthrough.
- KDE Plasma desktop branded as ArkaOS: top panel + floating dock +
  full-screen launcher layout, dark + green design system throughout.
- Arka Design System v1.0 (`arka-design-system/`): tokens, typography,
  motion, components, privacy color mapping.
- Custom SDDM login theme, Plymouth boot animation (▲ → glow → ARKA),
  Arka icon pack (12 icons), ArkaCursor, Arka sound theme, generated
  signature wallpaper, Inter/JetBrains Mono typography.
- First-party apps: Dashboard (privacy score, timeline, weekly report),
  Capsule (app store), Permissions, Settings, WiFi, Sound, Update, Welcome.
- Everyday features: power profiles, rich file previews, Bluetooth (bluedevil),
  KRunner search, Klipper clipboard.
- First-boot wizard (user creation, autologin choice) and first-login
  branding one-shot.
- Developer docs (`docs/`): architecture, building, testing. QA boot-loop
  harness (`scripts/qa-boot-loop.sh`).

### Fixed
- KDE package transaction failure on bootc base (`rootfiles` exclusion).
- Transient Fedora mirror failures during the 1500-package KDE layer
  (dnf retries=25 + fastestmirror).
- Small VM display (640×480) — first-login resolution bump via kscreen-doctor.
- GUI hangs in hardware-less VMs (all hw tool calls bounded by timeout).

### Known Issues
- GRUB is the active bootloader; measured UKI boot (PCRs 11–15) blocked until
  bootupd gains a systemd-boot component upstream.
- No per-file fs-verity pinning in composefs yet (structural immutability only).
- GTK apps (Adwaita) and Qt apps (Breeze) match in color/font but not in
  every widget metric — consistency audit in progress.
- Tested only in QEMU/KVM (virtio-vga); no bare-metal hardware matrix yet.
