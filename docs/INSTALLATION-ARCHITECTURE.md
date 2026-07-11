# Installation Architecture

Where installation is today, where it's going, and the rules that keep it clean.

## The invariant

Everything derives from **one source image** (`localhost/arkaos:dev`, the
Containerfile build), and the **first-boot wizard never changes**:

```
Container image (Containerfile — the OS)
        │
        ▼
bootc-image-builder
        │
        ├── qcow2   (DP1 — QEMU/KVM, shipping today)
        ├── raw     (DP2 — dd to disk / USB)
        └── iso     (DP2 — live installer media)
        │
        ▼
Installation (whatever the medium)
        │
        ▼
arkaos-firstboot (OOBE: user, autologin) — identical on every path
        │
        ▼
First login (arka-plasma-firstrun: branding, layout) → Desktop
```

However the bits land on the disk, the system that boots afterwards is
byte-identical and goes through the same OOBE. Installation is delivery,
not configuration.

## DP1 (now): qcow2 only

`bootc-image-builder --type qcow2` deploys the image into a ready-to-boot
disk file (it runs `bootc install to-filesystem` inside its osbuild pipeline).
There is no installer; nothing to install — the qcow2 *is* the installed
system, pre-OOBE. Audience: developers with QEMU/KVM.

## DP2 (planned): real hardware

```
USB → boot live environment → Arka Installer → bootc install to-disk
    → reboot → firstboot wizard → desktop
```

The installer is deliberately thin. Its entire contract:

1. Ask **which disk** (list disks, human names — "500 GB SSD", not /dev/nvme0n1).
2. Confirm **erase**.
3. Run `bootc install to-disk`.
4. Reboot.

### Rules for the installer

- **No custom partitioning engine.** bootc owns deployment; the installer
  is a front-end over one command. Partitioning UI ("alongside Windows",
  "advanced") is deferred until the OS is mature — and even then it
  parameterizes bootc/OS tooling, never reimplements it.
- **No Arka internals knowledge.** The installer must not know about arkad,
  Plasma, or the design tokens' existence — only "write this immutable image
  to that disk." If the installer needs to know something about the OS,
  that something belongs in the image or the OOBE instead.
- **No plumbing vocabulary in the UI.** Users never see the words qcow2,
  raw, bootc, osbuild, ostree, or composefs. The user journey is:
  Download → Flash USB → Boot → Install → Restart → Welcome to ArkaOS.
- **Arka-branded, design-system compliant** — same tokens, same typography,
  same motion rules as the rest of the OS (see `arka-design-system/`).

### Why not Anaconda (`--type anaconda-iso`)?

It works today and costs nothing, but the installer UI is stock
Fedora-branded Anaconda — a foreign body in an OS whose identity is
coherence. Acceptable as an internal stopgap for hardware bring-up testing;
not acceptable as a shipped artifact.

## Sequencing

1. DP1 ships qcow2. (Feature-frozen, QA in progress.)
2. Hardware bring-up: `--type raw`, dd to a test machine, fix what breaks
   (firmware, GPU, WiFi). No installer yet — raw image is the vehicle.
3. Live ISO + thin Arka Installer as described above.
4. Only after the OS is validated on hardware: smarter installer options.
