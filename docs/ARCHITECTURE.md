# ArkaOS Architecture

Why each piece exists, how they talk to each other, and where to change things.
Read this before touching anything.

## The one-paragraph version

ArkaOS is a privacy-first desktop OS built as an **OCI container image** (bootc).
The rootfs is immutable at runtime (composefs/ostree); updates are atomic image
swaps with rollback. A Rust daemon (**arkad**) enforces privacy invariants
continuously and exposes state over D-Bus. The desktop is **KDE Plasma** branded
into the Arka design system, with a set of first-party GTK4 privacy apps
(dashboard, capsule, permissions, settings) that talk to arkad. Firefox runs
inside a bubblewrap sandbox that hides the real home, /etc secrets, and D-Bus.

## Why these foundations

**Why bootc / immutable?** The privacy promise is only as strong as the
integrity story. An OS defined by a `Containerfile` is reproducible, diffable,
and atomic: an update either fully lands or doesn't exist. Rollback is free.
Malware can't persist in a rootfs it can't write to (composefs mounts it
read-only). This is the same architectural bet GrapheneOS makes on Android's
verified boot — we take the closest desktop equivalent.

**Why Fedora (fedora-bootc:42)?** Newest kernel/systemd/Plasma with a
first-class bootc story. We tried CentOS Stream 10 first; it lacked sway/KDE
freshness and its kernel misses CONFIG_FS_VERITY (see PHASE3-FINDINGS.md).

**Why KDE Plasma, not a custom compositor?** We built a Hyprland shell first
(phases 5–12) and hit the long tail: window controls, session management,
lock screen, notifications, polkit, screen sharing… A compositor is a decade
of edge cases. Plasma gives us all of it, themable to the pixel, while we keep
our identity via the design system, custom SDDM/Plymouth themes, panel layout
scripting, and our own apps. "ArkaWM" today = branded KWin. A from-scratch WM
is deliberately deferred until the product is validated.

**Why D-Bus?** It's the desktop lingua franca. arkad exposes
`org.arka.arkad` on the system bus; shell apps consume it with typed enums
(`arka-shell-common`) that degrade gracefully (`Unknown(String)`) so old shells
survive new daemons.

## Components

```
Containerfile            — THE OS. Every byte of the image is declared here.
arkad/                   — privacy daemon (Rust, static musl, no GTK)
  src/enforcers/         — mac, dns, hostname, ipv6, sandbox: one invariant each
  src/ipc/               — D-Bus interface (org.arka.arkad)
  src/score.rs           — privacy score aggregation (0–100)
arka-shell/              — GTK4/libadwaita first-party apps (one crate each)
  arka-shell-common/     — design tokens (theme.rs) + typed IPC contract
  arka-dashboard/        — privacy score, enforcer status, timeline, weekly report
  arka-capsule/          — app store front-end (flatpak)
  arka-perms/            — permission manager
  arka-settings/ etc.    — settings, wifi, sound, update, welcome, hotkeys
arka-design-system/      — canonical design spec v1.0 (tokens, motion, rules)
arka-icons/              — Arka icon theme (SVG, inherits breeze-dark)
plymouth-theme-arkaos/   — boot splash (script plugin)
sddm-theme-arkaos/       — login screen (QML)
desktop-files/           — launcher entries for the arka apps
arka-layout.js           — Plasma scripting: top panel + floating dock layout
arka-plasma-firstrun     — first-login one-shot: wallpaper, scheme, layout
arkaos-firstboot         — first-boot TUI: user creation, autologin choice
```

## How arkad works

Single-threaded main loop (no tokio — nothing here needs async):
enforce all invariants on start, then re-verify every 60 s and re-enforce on
drift. Every enforcement/drift/recovery is appended to
`/var/log/arkaos/privacy.jsonl` — that file is the source of truth for the
dashboard timeline and weekly report. Enforcement goes through well-known CLI
tools (nmcli, hostnamectl, sysctl, resolved drop-ins), never bespoke netlink:
auditable and boring on purpose. Config: `/etc/arkad/arkad.toml`
(serde+toml, secure defaults baked in; runs with no file present).

## The browser sandbox

`/usr/bin/firefox` → wrapper → bubblewrap → `/usr/bin/firefox-unwrapped`.
Home is a tmpfs (real home invisible), /etc is an allowlist (no NetworkManager
creds, no arkad config, no machine-id), /run is a tmpfs (no D-Bus), only
~/Downloads is re-exposed rw. Wayland socket is passed through. Baked at build
time; composefs makes the wrapper unremovable at runtime. Verified by proof:
a sentinel file in the real home must be unreadable inside (`firefox --shell`
runs the identical bwrap args for headless testing). See PHASE4-SANDBOX.md.

## Desktop branding pipeline (build → first login)

1. **Build time (Containerfile):** ArkaOS.colors (Plasma scheme), kdeglobals in
   /etc/skel (scheme, accent 22,199,132, Inter, icons=Arka, sounds=Arka,
   cursor via kcminputrc), SDDM theme, Plymouth theme + initramfs rebuild,
   wallpaper + icons + cursor + sounds generated/copied, KWin effects config.
2. **First boot (arkaos-firstboot):** user creation, optional SDDM autologin,
   `systemctl set-default graphical.target`.
3. **First login (arka-plasma-firstrun):** VM resolution bump, wallpaper +
   scheme apply via plasma-apply-*, panel layout via
   `PlasmaShell.evaluateScript(arka-layout.js)` (top bar 36px + floating
   icons-only dock + kickerdash full-screen launcher), then self-removes.

Why a first-login script instead of baking plasmashell config? Plasma's panel
config is per-user, versioned, and hostile to hand-written files; the scripting
API is the supported path. dbus-send is used because qdbus isn't in the image.

## Design system

`arka-design-system/README.md` is canonical. Two enforcement points:
- GTK apps: `arka-shell-common/src/theme.rs` (TOKENS + ADW_OVERRIDES css).
- Qt/Plasma: `ArkaOS.colors` + kdeglobals.
A token change happens in the spec first, then both implementation layers.

## Boot chain

GRUB (bootupd; systemd-boot blocked upstream, see PHASE3-FINDINGS.md) →
UKI-capable kernel + initramfs with Plymouth (arkaos theme, rhgb quiet) →
composefs root pivot → systemd → sddm.service (arkaos QML theme) →
Plasma Wayland session → arkad already active (system service since early boot).

## Known platform limits (accepted, documented)

- PCRs 11–15 / measured UKI boot: needs systemd-boot as active bootloader;
  bootupd on this base has no systemd-boot component. PCRs 0–10 measured today.
- No fs-verity in el10-era investigation; Fedora 42 kernel has it but per-file
  verity pinning in composefs isn't wired yet (structural immutability only).
