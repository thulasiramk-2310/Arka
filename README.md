<p align="center">
  <img src="assets/branding/arka-wordmark.svg" width="300" alt="ARKA: your computer is yours"/>
</p>

<p align="center">A desktop Linux where privacy settings are on by default, and the system tells you plainly what it checked and what it didn't.</p>

<p align="center"><strong><a href="https://thulasiramk-2310.github.io/Arka/">Website</a></strong> · <a href="https://thulasiramk-2310.github.io/Arka/#tour">One-minute tour video</a> · <a href="https://thulasiramk-2310.github.io/Arka/docs/">Docs</a></p>

<p align="center">Version 0.1.0-dp1 · Developer Preview · GPL-3.0</p>

<p align="center">
  <img src="assets/branding/hero.svg" width="100%" alt="ArkaOS layers from top to bottom: you, the desktop, arkad (the privacy daemon), the immutable image, the Linux kernel, hardware"/>
</p>

---

## What is ArkaOS?

ArkaOS is a desktop Linux we are building around one idea: the privacy
settings that matter should already be on when you first log in, and the
system should say clearly what it checked.

It is not a new kernel and not a distribution from scratch. It is Fedora's
bootc image with KDE Plasma on top, a small Rust daemon called
[`arkad`](arkad/), a few apps of our own, and a sandboxed Firefox.

The OS ships as one image. The root filesystem is read-only while the system
runs. An update swaps in a whole new image, and the previous one stays so you
can go back. `arkad` applies a handful of privacy settings and checks them
again about once a minute. When it says a setting is *Configured*, that
setting is in place. It does not mean your apps or your network traffic are
private.

> **Read this first.** ArkaOS is a developer preview. Don't rely on it to
> protect anything important yet. It has booted on one real laptop, the
> installer has only been tested in a VM, and nothing here is a security
> guarantee. See the [threat model](docs/THREAT-MODEL.md) and
> [SECURITY.md](SECURITY.md).

## Developer Preview 1

DP1 is the first thing we released: a qcow2 disk image for QEMU/KVM, for
developers, with no installer. Everything below was tested in a VM. The last
column says where else we have seen it work.

We released it, but its exit checklist
([docs/DP1-EXIT-CRITERIA.md](docs/DP1-EXIT-CRITERIA.md)) was never filled in.
So we can't show that every item on it was met.

| Part | What it is | Seen working |
|---|---|---|
| Read-only root (composefs) | The system files can't be changed while it runs | VM |
| Image updates | The whole image is swapped, the old one is kept | VM |
| `arkad` | Daemon that applies and re-checks privacy settings | VM |
| Privacy Dashboard | Lists each setting and when it was last checked | VM |
| Browser sandbox | Firefox runs inside bubblewrap | VM |
| Permissions app | Switches for the direct grants of Flatpak apps | VM |
| Capsule | App store front end for Flathub | VM |
| KDE Plasma look | ArkaOS theme, icons, cursor, sounds, wallpaper | VM, one laptop |
| Boot animation and login screen | Plymouth and SDDM themes | VM |
| First-boot setup | Creates your account | VM |
| Wi-Fi, Bluetooth, power profiles | KDE's own tools | Included in the image; Wi-Fi seen on one laptop |
| Search, clipboard, file previews | KDE's own tools | VM |

Details: [Release Notes](docs/RELEASE-NOTES-DP1.md) · [Changelog](CHANGELOG.md)

## Since DP1: work toward DP2

This is source work plus the test results we wrote down along the way. The
rollback and installer results (2026-10-11 and 2026-10-12) are written up in
[Release Validation](docs/RELEASE-VALIDATION.md). The other dated results come
from our own test sessions and are not all recorded in a repo document. Everything
here was tested in a VM between 2026-10-11 and 2026-10-26 unless it says otherwise. "Not yet on hardware" means
exactly that: we haven't tried it.

### Real hardware

- **First boot on a real laptop.** An Intel i5-10210U laptop booted ArkaOS from
  a USB stick, with the internal disk untouched: GRUB, then the KDE Plasma
  desktop. Wi-Fi, touchpad, display, brightness and suspend/resume worked, and
  `arka-pulse` read a real temperature sensor. That is one machine, so treat it
  as a first data point.
- **Fixes from that test.** The disk image no longer turns on a serial console,
  which was crash-looping the login there. `scripts/flash-usb.sh` writes and
  verifies USB sticks and repairs the GPT. Browser audio socket access and
  Capsule's user-level Flathub setup were fixed in the source. None of these is
  confirmed on the laptop yet. A Capsule install from Flathub did complete in a
  VM.
- **arka-pulse.** A read-only *System Health* view (`org.arka.pulse`, like
  `arkad`). It only watches, and it says what it can't measure yet. See the
  [design](docs/RELIABILITY-ARKA-PULSE.md).

### Installing, updating, rolling back

- **Installer ISO with disk encryption (first version).** `scripts/make-iso.sh`
  builds an installer from the image using Fedora's installer (Anaconda), set up
  in `installer/config.toml`. You pick the disk, nothing is written before
  "Begin Installation", and the disk is encrypted (LUKS2) by default. ArkaOS
  setup creates your account on first boot. Tested in a VM on 2026-10-12:
  install, unlocking at boot, setup with all six settings configured, the
  desktop, and automatic rollback after a broken update. Known problems: the
  installer still shows Fedora's branding, it asks for the passphrase twice, the
  unlock prompt shows the disk's internal name, every restart of an encrypted
  computer (rollback retries included) needs the passphrase, and the 6.7 GB ISO
  is not published. Not tested on real hardware.
- **Signed updates from ghcr.io.** GitHub Actions builds the image on pushes to
  `master` that touch more than docs, the website, assets or Markdown files, and
  once a week. It splits the image into
  small layers, signs it with cosign and publishes it as
  `ghcr.io/thulasiramk-2310/arka`. ArkaOS accepts an image from there only if it
  is signed with the ArkaOS key (`cosign.pub`). In a VM on 2026-10-17 an unsigned
  image and an image checked against another key were both refused before
  anything was downloaded, and the signed image installed and booted. A later
  update downloaded 5 of the 65 layers (863 MB instead of 5.3 GB). New installs
  take their updates from there. Not yet on hardware.
- **Updates never restart the computer on their own.** They download in the
  background. The Updates app shows the installed version, any downloaded
  update, whether a restart is needed and whether a previous version is
  available. You choose when to restart.
- **Automatic rollback after a bad update (greenboot, with two fixes).** After
  you restart into a new version, start-up checks run. If the version fails
  them, or fails before the desktop starts, the computer restarts and, after
  three failed boots, goes back to the previous version. Stock greenboot 0.16.4
  never recorded which version to go back to, so ArkaOS fixes that. It also
  arms GRUB's boot counter when an update is staged, so a kernel or early-boot
  failure falls back too, instead of waiting at a recovery screen.
  - VM, 2026-10-11: a broken update went back after three failed boots, an
    update that could not start went back within about 30 seconds, a healthy
    update stayed across restarts, `sudo bootc rollback` still worked by hand,
    and the test files in `/home` and `/var` were unchanged.
  - One real laptop (i5-10210U, USB stick), 2026-10-12: the broken update went
    back after three failed boots and the healthy update stayed across two
    restarts. An update that fails before the system starts has not been tried
    on hardware.
  - If applying an update fails at shutdown (for example a full `/boot`), ArkaOS
    keeps the version you were on. Stock greenboot would roll that healthy
    system back to an older one
    ([greenboot-rs#219](https://github.com/fedora-iot/greenboot-rs/issues/219)).
  - The installer gives `/boot` its own partition. On 2026-10-17 a broken update
    rolled back on such an install even with the ArkaOS greenboot fix switched
    off, because the trigger is stopped together with `/boot` there (reported as
    [greenboot-rs#218](https://github.com/fedora-iot/greenboot-rs/issues/218)).
    USB images have no separate `/boot` and rely on the fix.

### Privacy and security

- **Privacy Dashboard without a score.** It lists each setting as *Configured*
  or not, says how long ago it was checked, and has a page for updates and for
  going back to the previous version.
- **How we word privacy.** Firefox's tracking and fingerprinting-script blocking
  reduce some tracking. They do not prevent fingerprinting. *Configured* means
  the checked setting is in place, not "you are protected". A check that is
  missing or stale shows as unknown. See the [threat model](docs/THREAT-MODEL.md).
- **Firewall on by default.** An ArkaOS firewall zone rejects connections other
  computers try to open to this one. `arkad` checks the zone's actual rules and
  policies, not just its name. Setup, the dashboard and the tray show the result.
- **Encrypted DNS, with one exception.** On a network that blocks encrypted DNS
  (some hotels and sign-in pages), the dashboard notices that names aren't
  resolving and offers to allow unencrypted DNS **on that network only**. It then
  shows *Off on this network*, and other networks stay strict.
- **Kernel hardening.** Stricter kernel boot options (memory wiping, lockdown in
  integrity mode, and others) and some runtime settings. User namespaces stay
  on, because Firefox's sandbox and Flatpak need them.
- **Fewer false alarms.** The tray only warns about a problem that lasts about
  30 seconds, so a firewall that is slow to start, or a setting `arkad` fixes at
  once, no longer shows a popup. When a setting comes back on its own, the log
  says *Setting confirmed again*. Tested in a VM on 2026-10-14.
- **KDE Connect is off by default.** While it runs, it makes the computer
  visible to phones and computers on the same network. It no longer starts at
  login, from the tray or from Dolphin's right-click menu. Settings › Internet
  has a switch that starts it now and at every login, and turns it off again.
  Opening the KDE Connect app also turns it on until you log out. Tested in a VM
  (2026-10-20 and 2026-10-25): it started and stopped, port 1716 opened and
  closed, and it came back after logging in again. Not tested with a real phone.

### Desktop

- **New setup and a Light/Dark look (Fedora 44 candidate).** A rewritten
  first-boot setup: choose Light or Dark, create your account, see where each
  privacy setting stands right now, then sign in. The login screen, desktop,
  icons and every ArkaOS app follow that choice, and Settings › Appearance
  switches it later. The first desktop starts in the ArkaOS look, and the boot
  logo stays centred when the display changes resolution. Tested in a VM in both
  looks. Not yet on hardware.
- **Dock.** A dock at the bottom replaces the taskbar: pinned and open apps,
  icons that grow under the pointer, a small label with the app's name, a dot
  under running apps, a divider before Trash, and a bounce while an app starts.
  The desktop behind it is blurred on computers whose graphics run KDE's blur
  effect (not in a VM without 3D). It hides only when a window covers it and
  comes back at the bottom edge. Right-click it to keep it always visible or to
  change apps and sizes. It is
  [Crystal Dock](https://github.com/dangvd/crystal-dock) with an ArkaOS patch
  (see `crystal-dock/`). Full-screen apps hide it, and it does not come back at
  the edge until you leave full screen. Tested in a VM (2026-10-18). Not yet on
  hardware.
- **Launcher.** The button at the top left, or the Meta key, opens a panel
  instead of KDE's full-screen menu: a search box for apps and settings, the app
  categories, your pinned apps, and Lock, Log Out, Restart and Shut Down.
  Right-click an app to pin or unpin it. The line at the bottom shows the same
  privacy status as the tray and opens the Privacy Dashboard. The app list and
  search are Plasma's own, so newly installed apps show up, and the panel
  follows the Light or Dark look. New accounts get it. An older account keeps
  KDE's menu until you right-click it and pick *Show Alternatives*. Tested in a
  VM (2026-10-20) with mouse, keyboard and the Orca screen reader, in Light and
  Dark, at 150% scaling, and on two virtual screens. Not yet on hardware.
- **Top bar on every screen.** Each screen gets the bar with the launcher, clock
  and tray, including a screen plugged in later. A bar you remove from a screen
  stays removed. Tested in a VM with two virtual screens (2026-10-20). Not yet
  with real monitors.
- **Window snapping.** Meta+Left or Meta+Right puts a window on half of the
  screen. Dragging it to a screen edge shows a preview. This is KDE's own tiling,
  unchanged. Tested in a VM (2026-10-25) with the keyboard and by dragging. Not
  yet on a touchpad.
- **Move the dock.** Settings › Appearance has Bottom, Left and Right. Crystal
  Dock has no menu entry for this, so Settings changes its setting and restarts
  it. Tested in a VM (2026-10-25): each choice moved the dock and one dock kept
  running.
- **Popups with cut corners.** Notifications, the volume and brightness popups,
  the calendar, the tray and the launcher have corners cut at 45 degrees, like
  the peaks in the ArkaOS mark and wallpaper, with a thin border and a soft
  shadow. The launcher's search field, highlights and tiles match. It is part of
  the ArkaOS Plasma style, so it follows Light and Dark. Tested in a VM
  (2026-10-20).
- **Lock screen in Light.** The lock screen keeps the dark wallpaper, as the
  login screen does, so its white clock stays readable. New accounts' trays
  leave out Weather and Vaults. Both can be switched back on in the tray's
  settings. Tested in a VM (2026-10-20).
- **App icons.** One family with a distinct colour per app. Firefox and the file
  manager keep their own icons.
- **Night light.** Settings › Appearance has a switch for warmer screen colours
  from 8 pm to 6 am. KDE's own schedule looks up your location by default. This
  one uses fixed times, so the switch asks for no location (we read this in the
  schedule library's source and did not check it with a network trace). Tested
  in a VM (2026-10-25): KWin reported Night light at 4500 K when on and 6500 K
  when off.
- **Reduce motion.** Settings › Appearance has a switch that turns off most
  window, menu and desktop animations, using KDE's own animation setting.
  Turning it off returns to KDE's normal speed. Tested in a VM (2026-10-25): the
  setting is written and removed, KDE's change notice is sent on the session bus,
  and Settings shows the real value when reopened. We did not time the
  animations.
- **Natural touchpad scrolling by default** (content follows the fingers). It
  can be turned off in System Settings. Tested in a VM with a virtual touchpad.
  Not yet on a real one.
- **Installing apps shows what they can reach first.** `arka install NAME`, and
  Capsule's Install button, print what a Flathub app asks for in plain words
  (all your files, every device, the internet and so on) and ask before
  installing it for your account only. The text comes from Flathub's published
  metadata. It limits nothing by itself and does not make an app safe. We read
  the prompt and permission text against Flathub from the build machine and did
  not run a full install. Not tested in a VM or on hardware.
- **Keyboard shortcuts page.** Meta+/ opens a list of the ArkaOS shortcuts and
  KDE's window, split-screen and space shortcuts. Each key on it was checked
  against KDE's shortcut settings or is set by an ArkaOS component. Not tested
  with real key presses.
- **Three spaces.** New accounts start with three virtual desktops and the top bar
  shows KDE's space switcher. Older accounts get two more once, at login. We have
  not seen the switcher working: our test session has no window manager. Not
  tested in a VM or on hardware.
- **Computer name and wheel tutorial in setup.** Setup now asks for a computer
  name (default `arka`, which is the same on every ArkaOS computer) and ends with
  a short practice of the format wheel. We checked each page as a screenshot of
  the real window. It has not run at a real first boot.

### Working with files

- **Convert files from the right-click menu.** In Dolphin, *Convert to* turns
  pictures into PNG, JPEG, WebP or PDF, a PDF into one PNG per page or a smaller
  PDF (pictures reduced to 150 dpi; nothing is kept if it would not be smaller),
  sound into MP3, FLAC, WAV, Ogg, Opus or AAC, and video into MP4 (H.264), MKV or
  WebM, or just its sound as MP3. The copy is saved next to the original, or in
  Downloads if that folder is read-only, and never replaces a file. Each converter (ffmpeg, Pillow, poppler, Ghostscript)
  runs in a sandbox with no network access. Long sound and video conversions
  show progress, and stopping one leaves no half-made file behind. Tested in a VM
  (2026-10-19 and 2026-10-20). Not yet on hardware. Dolphin's preview of a very
  short H.264 video (about 2 seconds) can look scrambled. The file itself is fine.
- **Remove hidden details.** Photos, PDFs, sound and video files often carry
  details you can't see: where a photo was taken, the camera, the author's name,
  the software that made a document. *Remove hidden details* in Dolphin's
  right-click menu saves a copy without them, made by
  [mat2](https://0xacab.org/jvoisin/mat2) inside the same no-network sandbox. It
  works on PNG, JPEG, WebP, GIF, BMP, TIFF, PDF, MP3, FLAC, Ogg, WAV, MP4 and AVI
  files. It removes what mat2 knows about. It cannot remove what is in the
  picture or sound itself. Pictures are saved again, which can change a JPEG
  slightly. Tested in a VM (2026-10-20): a photo's camera and author, a video's
  GPS position and a PDF's author were gone from the copies.
- **Format wheel and edit wheel.** Drag a file, press Meta+Alt+C, and a ring of
  the formats that file can become appears at the pointer. Drop the file on one
  to convert it. Meta+Alt+E shows a ring of edits instead. *Convert with the
  format wheel* and *Edit with the edit wheel* in the right-click menu open the
  same rings for clicking. The ring is a flat octagon with cut corners; the choice
  under the pointer is outlined, the middle shows the file's name, and a short
  green tick on the middle's edge points at the choice. The shortcuts can be changed in System Settings › Shortcuts.
  Tested in a VM (2026-10-26) with a real drag from Dolphin: the format wheel
  converted a picture to WebP and left the original alone, and the edit wheel
  opened the crop window. Drags from Gwenview and Firefox were tested on
  2026-10-19. That was the earlier round look; the octagon look (2026-10-27) has
  only been checked in screenshots. Not yet with a real touchpad.
- **Edit from the right-click menu.** For a picture: *Crop* (drag the corners, or
  pick 1:1, 16:9, 3:2, 4:3 or 3:4), *Black out parts*, *Add a background* (a
  colour or gradient, and how much room to leave) and *Rotate left* or *right*.
  For PDFs: *Merge into one PDF* (select two or more) and *Split into pages*. For
  sound and video: *Trim* (type a start and an end). The result is a copy next to
  the original and never replaces a file. The work runs in the same no-network
  sandbox as converting. *Black out* replaces the pixels in the copy, so what was
  under a box can't be read back from the copy. It does not touch the file's name
  or its hidden details (use *Remove hidden details* for those).
  Tested in a VM (2026-10-26) with a real mouse and keyboard: the menu entries for
  rotate right and merge, and the crop (opened from the edit wheel), add a
  background and trim (MP3) windows. The other menu entries were seen but not
  clicked. Split into pages and trimming an MP4 ran in the VM from the command
  line. Black out's drag was checked in the VM, but its button was only clicked
  outside it. WebM trimming was checked on the build machine. The other sound and
  video formats were not tested. Not yet on hardware.

### For developers

- **VARA and Git.** Under *Advanced options*, setup can install
  [VARA](https://github.com/thulasiramk-2310/vara) (version control with built-in
  undo and integrity checks) and set up Git with the name and email for your
  commits. Settings › Developer tools changes them later and can run a VARA Hub,
  a web view of your repositories that only this computer can reach. The Hub is
  off until you switch it on. VARA is built from its v0.4.4 release. Tested in a
  VM (2026-10-20). Not yet on hardware.

Still a developer preview: the installer is new and VM-tested only, we have one
hardware data point, and the known rough edges are in the release validation notes and the field notes.

---

## Architecture

<p align="center">
  <img src="assets/branding/architecture.svg" width="100%" alt="Component map: first-party apps and the Plasma desktop on top, arkad on the right publishing read-only status over the system D-Bus to the Dashboard, tray and first-boot setup, the immutable bootc image underneath, then the Linux kernel"/>
</p>

Why bootc, why Plasma, why D-Bus, and why we don't write our own compositor:
see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## The privacy model

<p align="center">
  <img src="assets/branding/privacy.svg" width="100%" alt="Three separate pieces: the Firefox wrapper and bubblewrap, the Permissions app and flatpak override, and arkad's checks shown through D-Bus in the Dashboard and tray. No single piece sees every app or connection."/>
</p>

There is no single gatekeeper. These are three separate pieces.

**The browser has a restricted view of the filesystem.** The wrapper hides your
real home, except `~/Downloads`, which the browser can read and write. It
exposes a short list of system files plus the Wayland and audio sockets. Audio
access can include microphone recording. The session D-Bus is hidden. Our
earlier isolation checks are in [PHASE4-SANDBOX.md](PHASE4-SANDBOX.md). Each
release still needs to be tested again.

**The Permissions app edits Flatpak settings.** It changes the direct grants of
installed Flatpak apps (network, audio, devices, home, Downloads, Wayland).
Portals and other access paths are separate, so a switch here is not a complete
picture of what an app can reach.

**`arkad` checks settings.** It tries to apply MAC randomization, strict
DNS-over-TLS, a generic hostname, IPv6 temporary addresses and the ArkaOS
firewall zone, then checks them again about every minute. A per-network DNS
exception is shown as an exception, never as encrypted. Checks can fail, and a
configured DNS policy does not prove that every app uses it. The tray counts a
check as current only if it finished less than three 30-second polls ago.

OS rollback picks another system image. It does not restore older personal
files, and it can't promise the older image will boot.

## One image, three ways onto a disk

<p align="center">
  <img src="assets/branding/installation.svg" width="100%" alt="One container image becomes a qcow2 for VMs, a raw disk image for a USB stick, or an installer ISO that encrypts the disk by default. All three lead to the first-boot ArkaOS setup. The image is also published to ghcr.io, where installed systems get updates."/>
</p>

All three start from the same container image, and all three end in the same
first-boot setup. The ISO's installer also edits `/etc/fstab` and points the
system at the published image, so the installed systems are not byte-for-byte
the same. The qcow2 and raw images come out of different tools, and only the
qcow2 is the DP1 release. The strategy doc is partly out of date. See
[docs/INSTALLATION-ARCHITECTURE.md](docs/INSTALLATION-ARCHITECTURE.md).

## How it's built

<p align="center">
  <img src="assets/branding/workflow.svg" width="100%" alt="Three of our GitHub Actions workflows: Image (build, split into layers, push to ghcr.io, sign), Source checks (shell, Python and Rust tests), and Stick image (run by hand: verify signature, install to a raw disk, keep for 3 days)"/>
</p>

GitHub Actions builds and signs the image and runs the source tests. It does
not boot the image. We do that by hand in a VM. Our rule is that nothing counts
as done until we have watched it work in a booted VM, because **a build that
compiles is not a build that works**. For security claims we also check the
exact deployed file, path and user. `scripts/qa-boot-loop.sh` boots an image
many times and records failures. Our target was 100 of 100 clean boots before
DP1. [docs/DP1-EXIT-CRITERIA.md](docs/DP1-EXIT-CRITERIA.md) lists it as not
checked off, and we have no recorded run of it, so we can't say it was met. See
also [docs/TESTING.md](docs/TESTING.md).
To build it yourself, see [docs/BUILDING.md](docs/BUILDING.md).

## Screenshots

Fedora 44 candidate, light look, from a VM test of a built image:

<p align="center">
  <img src="website/img/desktop-light.png" width="100%" alt="The ArkaOS desktop in its light look: slim top bar, the ArkaOS Dock with the app menu, Settings, Discover, Files, Firefox and, after a divider, Trash, and a pale wallpaper with the ArkaOS triangle in thin lines"/>
</p>
<p align="center">
  <img src="website/img/dock-light.png" width="60%" alt="Close-up of the ArkaOS Dock: icons grow around the pointer, a small label above the Files icon shows its name, Dolphin, and a dot under it shows it is running"/>
</p>
<p align="center">
  <img src="website/img/setup-privacy-light.png" width="49%" alt="The last step of setup, Your privacy settings, listing six settings including the firewall, each marked Configured"/>
  <img src="website/img/dashboard-light.png" width="49%" alt="The Privacy dashboard listing six settings, each marked Configured, with when they were last checked"/>
</p>

<details>
<summary><strong>Historical DP1 screenshots (outdated)</strong></summary>

The dashboard score and report's DNS coverage shown below are obsolete
indicators, not evidence of current protection or measured traffic.

<p align="center">
  <img src="assets/screenshots/dp1/desktop.png" width="100%" alt="ArkaOS KDE Plasma desktop: branded ARKAOS wallpaper reading 'your computer is yours', a slim top bar with launcher, system tray and clock, and a floating icons-only dock"/>
</p>

<p align="center">
  <img src="assets/screenshots/dp1/lockscreen.png" width="49%" alt="ArkaOS lock screen: KDE kscreenlocker showing a large clock and date over the branded ARKAOS wallpaper"/>
  <img src="assets/screenshots/dp1/wizard.png" width="49%" alt="ArkaOS first-boot welcome wizard: privacy feature list (composefs, DoT DNS, MAC random, IPv6, bwrap) beside an account-setup prompt"/>
</p>

<p align="center">
  <img src="assets/screenshots/dp1/launcher.png" width="100%" alt="ArkaOS full-screen application launcher (kickerdash): search, favorites, application categories and power actions over the ARKAOS wallpaper"/>
</p>

<p align="center">
  <img src="assets/screenshots/dp1/dashboard.png" width="49%" alt="Historical Privacy Dashboard with an obsolete 100/100 configuration score; not evidence of current protection"/>
  <img src="assets/screenshots/dp1/privacy-report.png" width="49%" alt="Historical Privacy Report with obsolete inferred DNS coverage; not a current measurement"/>
</p>

<p align="center">
  <img src="assets/screenshots/dp1/capsule.png" width="49%" alt="ArkaOS Capsule: first-party app grid plus an install catalog (Signal, Bitwarden, KeePassXC, ProtonVPN) served from Flathub"/>
  <img src="assets/screenshots/dp1/settings.png" width="49%" alt="ArkaOS Settings: General page with automatic login, ArkaOS version and hostname, and a category sidebar"/>
</p>

<p align="center"><em>Live DP1 captures (2026-07-11): desktop, KDE lock screen, first-boot
wizard, full-screen launcher, Privacy Dashboard, Privacy Report, Capsule, and Settings.</em></p>

</details>

## Roadmap

<p align="center">
  <img src="assets/branding/roadmap.svg" width="100%" alt="Roadmap: DP1 released (qcow2 for QEMU, VM only; its exit checklist is not filled in); DP2 in progress (real hardware, installer, signed updates; so far one laptop and a VM-tested installer); Beta, RC and 1.0 not started and not dated"/>
</p>

We don't have dates for Beta, RC or 1.0. DP2's final scope is not fixed yet;
it depends on what we learn from daily use ([DP2 notes](docs/DP2.md)).

## Repository

```
Containerfile            the OS: everything in the image is declared here
arkad/                   privacy daemon (Rust, static musl)
arka-pulse/              read-only System Health service
arka-shell/              GTK4 first-party apps, shared design tokens and IPC types
arka-design-system/      design spec v1.0: tokens, motion, components
arka-icons/              icon theme          plymouth-theme-arkaos/   boot splash
sddm-theme-arkaos/       login theme         desktop-files/           launcher entries
plasma-launcher/         launcher panel      plasma-style/            popups and panel style
panels/                  top bar on every screen
crystal-dock/            dock patch and build
convert/                 Convert to, edit and format wheel tools
arkaos-firefox           sandbox wrapper     arkaos-firstboot         first-boot setup
arka-plasma-firstrun     first-login branding one-shot
arka-apply-look          Light/Dark switch (first login and Settings)
systemd/                 update-status publisher for unprivileged apps
firewalld/               the ArkaOS firewall zone
kernel/                  kernel hardening (boot options and runtime settings)
installer/               installer ISO configuration
docs/                    architecture, building, testing, installation, release notes
scripts/                 QA harnesses, make-disk.sh, make-iso.sh, flash-usb.sh
```

## Documentation

| | |
|---|---|
| [Architecture](docs/ARCHITECTURE.md) | why each piece exists and how they talk |
| [Building](docs/BUILDING.md) | source to image to VM |
| [Testing](docs/TESTING.md) | how we verify, the QA scripts, budgets |
| [Installation](docs/INSTALLATION-ARCHITECTURE.md) | VM image, USB image and installer ISO (partly out of date) |
| [Threat model](docs/THREAT-MODEL.md) | what ArkaOS does and does not protect against |
| [Design System](arka-design-system/README.md) | tokens, typography, motion, components |
| [Release Notes](docs/RELEASE-NOTES-DP1.md) | what DP1 has, what is known to be incomplete |

## Security

Report problems privately through the repository's Security tab (*Report a
vulnerability*). We count wording that claims more protection than the system
gives as a bug too. Details are in [SECURITY.md](SECURITY.md).

---

<p align="center">
  <strong>ArkaOS</strong> · <a href="https://github.com/thulasiramk-2310">Thulasiram K</a> · GPL-3.0
</p>
