# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# ArkaOS — Privacy-first bootc Linux

## What this is
Privacy-first, user-friendly desktop Linux OS (the "GrapheneOS-for-desktop" gap).
Built on bootc immutable image model. Stack: NASM/C/Rust over a Linux LTS base.

## Environment
- Host: Arch Linux, user `Ram`, group `users`, home `/home/Ram`
- Build via podman-bootc machine `podman-machine-default` (qemu, rootful)
- Project dir: /home/Ram/arkaos/
- Container storage mount required: -v /var/lib/containers/storage:/var/lib/containers/storage

## Working pipeline (all green)
Build runs INSIDE the podman machine (virtiofs mounts /home/Ram at /var/home/Ram):
1. `podman machine ssh podman-machine-default "podman build --pull=newer -t localhost/arkaos:dev /var/home/Ram/arkaos/"`
2. `podman machine ssh ... podman run --rm --privileged -v /var/lib/containers/storage:/var/lib/containers/storage -v .../config.toml:/config.toml:ro -v .../output:/output quay.io/centos-bootc/bootc-image-builder:latest --type qcow2 --rootfs xfs localhost/arkaos:dev`
3. Boot test: `qemu-system-x86_64 -enable-kvm -m 2048 -cpu host -smp 2 -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd -drive if=pflash,format=raw,file=OVMF_VARS.4m.fd -drive file=output/qcow2/disk.qcow2,format=qcow2,if=virtio -nographic -serial telnet::4445,server,nowait -monitor telnet::4444,server,nowait -no-reboot`
4. Serial console: `telnet localhost 4445` — login ram/arkaos

## Containerfile structure
Multi-stage build:
- Stage 1: `rust:alpine` — compiles arkad statically (musl, `x86_64-unknown-linux-musl`)
- Stage 2: `centos-bootc:stream10` — installs arkad binary + unit, applies privacy configs

## arkad (Phase 2 — complete, verified)
Rust privacy daemon at `arkad/`. Four enforcers, all verified active in booted VM:
- `mac.rs` — writes /etc/NetworkManager/conf.d/00-arkaos-mac-random.conf, nmcli reload
- `dns.rs` — writes /etc/systemd/resolved.conf.d/99-arkad-dot.conf (DoT, Quad9 9.9.9.9)
- `hostname.rs` — hostnamectl set-hostname arka
- `ipv6.rs` — sysctl net.ipv6.conf.all.use_tempaddr=2

Main loop: enforce all on start, then re-verify every 60s and re-enforce on drift.
No tokio (uses thread::sleep). No nix crate. All enforcement via well-known CLI tools.
Config: /etc/arkad/arkad.toml — serde+toml with secure defaults baked in (works with no file).

## Solved issues
- Stale Arch keyring -> pacman -S archlinux-keyring
- libvirtd.socket is the unit (not .service)
- bootc-image-builder needs /var/lib/containers/storage mount
- Missing DefaultRootFs -> /usr/lib/bootc/install/00-defaults.toml with [install.filesystem.root] type="xfs"
- Output dir perms: clean from HOST side (virtiofs mount, can't chmod from inside machine)
- GRUB/edk2 page-fault: Fedora 42 GRUB crashes Arch's May 2026 edk2. Fixed by switching base to centos-bootc:stream10 — no bootloader override needed.
- `bootloader = "systemd"` in 00-defaults.toml fails in bootc-image-builder (bootupd must be in the osbuild env, not just the image). Default GRUB works fine on CentOS Stream 10.
- podman machine dies under memory pressure (two QEMU VMs + image build). Kill boot-test VMs before running bootc-image-builder.
- Multi-stage build: can't use sudo on host for musl toolchain; build arkad inside rust:alpine container instead.

## Phase 3 — Integrity & Boot Hardening (BANKED)

Status: composefs + TPM PCRs 0–10 live and verified. UKI/PCR 11–15 blocked by
CentOS Stream 10 platform limitation (bootupd lacks systemd-boot component; ESP
owned by bootc/ostree). NASM shim correctly avoided. See PHASE3-FINDINGS.md.

Fallbacks: `localhost/arkaos:grub-fallback`, `localhost/arkaos:phase3-milestone`,
`output/qcow2/disk-grub-fallback.qcow2`

Phase 4 (app sandboxing — browser isolation) is next.

### Details

### 3a: UKI (complete — built in image, GRUB still boots)
- `systemd-ukify systemd-boot-unsigned binutils` installed
- `/etc/kernel/install.conf`: `layout=uki` + `uki_generator=ukify`
- UKI pre-built at build time: `/usr/lib/modules/6.12.0-225.el10.x86_64/6.12.0-225.el10.x86_64.efi` (240MB)
- Sections verified: `.sbat .osrel .uname .linux .initrd` — all present
- **GRUB is still the active bootloader** — `bootloader = "systemd"` in 00-defaults.toml triggers
  "bootupd is required for ostree-based installs" because the BIB has bootupd-0.2.31-fc42 with
  only a `grub2-static` component, no systemd-boot component. CentOS Stream 10's
  `systemd-boot-unsigned` does not register a bootupd Loader component.
  To fix: either build a custom BIB with bootupd systemd-boot component, OR add a first-boot
  unit that runs `bootctl install` (which bypasses bootupd entirely).

### 3b: composefs integrity (investigated)
- composefs 1.0.8 ACTIVE as rootfs: read-only overlay with ostree object store as datadir
- `bootc status` shows `"composefs": null` — structural immutability only, no crypto pinning
- NO fs-verity: kernel 6.12.0-225.el10 lacks CONFIG_FS_VERITY (no /sys/fs/verity)
- NO dm-verity: no device mapper verity devices
- Gap: file content integrity (per-file verity digests in .cfs image) requires CONFIG_FS_VERITY
  in kernel + ostree/composefs built with --digest. Not available in el10 out of the box.

### 3c: TPM PCR measurements (complete, verified)
- `swtpm 0.10.1-2` installed on Arch host
- Boot QEMU with swtpm: add to QEMU cmd:
  `-chardev socket,id=chrtpm,path=/tmp/arkaos-tpm.sock -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0`
- swtpm start: `swtpm socket --tpmstate dir=/tmp/arkaos-tpm --ctrl type=unixio,path=/tmp/arkaos-tpm.sock --tpm2 --daemon`
- Initialize: `swtpm_setup --tpm2 --tpmstate /tmp/arkaos-tpm --createek --decryption --create-ek-cert`
- In VM: `/dev/tpm0` + `/dev/tpmrm0` present; `systemd-analyze pcrs` shows PCRs 0-10 + 14 measured
- `systemd-pcrphase` runs but dormant — gates on `ConditionSecurity=measured-uki`
  (requires booting via signed UKI with sd-stub extending PCR 11; not met with GRUB)
- Full measured-boot (PCRs 11-15 via systemd-pcrphase/pcrextend/pcrlock) activates
  once systemd-boot + UKI is the active boot path.

## Phase 4 — Browser Sandboxing (complete, verified)

Approach: bubblewrap (not Flatpak — image size, no runtime deps, headless-verifiable).
`/usr/bin/firefox` → symlink → wrapper; real binary at `/usr/bin/firefox-unwrapped`.
Baked at build time; composefs locks it read-only at runtime.

Sandbox: /home tmpfs (real home hidden), /etc allowlist (no NM creds, no arkad
config, no machine-id), /run tmpfs (no D-Bus), ~/Downloads bind-only re-exposure.
`firefox --shell` test mode uses identical BWRAP_ARGS array.

Verified: sentinel ~/secret.txt unreadable inside sandbox; /etc/NetworkManager and
/etc/arkad absent inside sandbox; arkad active; composefs ro.
See PHASE4-SANDBOX.md for full proof output.

## Style
Ram wants senior-level judgment calls, no hand-holding, brief direct answers.
