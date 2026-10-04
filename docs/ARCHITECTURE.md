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

**Why bootc / immutable?** Image deployments make system changes reviewable and
provide atomic staging and a previous deployment for recovery. Read-only system
files reduce routine modification; writable `/etc`, `/var` and user files remain
persistence surfaces. Composefs alone is not a signed, end-to-end verified boot
chain. Successful rollback and data preservation need release testing.

**Why Fedora bootc?** The current candidate targets Fedora 44 for its kernel,
systemd and Plasma integration. Earlier DP1 findings used Fedora 42; they do not
validate a new image automatically.

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

A Tokio loop attempts configuration on startup and checks each control on a
bounded 10–60 second interval. Manual and periodic reconciliation share a mutex;
subprocesses have timeouts. Failures stay unconfirmed and are retried. Selected
enforcement/drift/recovery events are appended to
`/var/log/arkaos/privacy.jsonl` — that file supplies the
dashboard timeline and weekly event counts, not traffic or DNS uptime.
Enforcement goes through well-known CLI
tools (nmcli, hostnamectl, sysctl, resolved drop-ins), never bespoke netlink:
auditable and boring on purpose. Config: `/etc/arkad/arkad.toml`
(serde+toml, secure defaults baked in; runs with no file present).

## The browser sandbox

`/usr/bin/firefox` → wrapper → bubblewrap → `/usr/bin/firefox-unwrapped`.
Home is a tmpfs (real home invisible), /etc is an allowlist (no NetworkManager
creds, no arkad config, no machine-id), /run is a tmpfs (no D-Bus), only
~/Downloads is re-exposed rw. Wayland and audio sockets are passed through;
direct audio-server access includes recording capability. The wrapper ships
in the system image, but administrators and other browser installations can
bypass it. Test its boundary with a sentinel file in the real home:
`firefox --shell` uses identical bwrap arguments. Earlier observations are in
PHASE4-SANDBOX.md; repeat them on each release candidate.

`LastCheck` is a read-only D-Bus Unix timestamp updated only after a complete
reconcile, including one with failed controls. Zero means no completed check.
The tray polls every 30 seconds and accepts ages strictly below 90 seconds;
missing, zero, future or older timestamps display a neutral unknown state.
“Configured” describes settings found in place, not a general protection claim.

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
