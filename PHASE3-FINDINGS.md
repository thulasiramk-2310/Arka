# Phase 3 Findings — Integrity & Boot Hardening

## 1. What's live (verified)

**composefs — read-only, integrity-backed rootfs**
- composefs 1.0.8 active as rootfs mount: `composefs on / type overlay (ro,...)`
- Directory tree is fixed by a `.cfs` erofs image; file data served content-addressed
  from the ostree object store. This is the bootc-native dm-verity equivalent.
- Structural immutability confirmed. No fs-verity or dm-verity needed on top of it
  for the immutable layer — composefs already provides that guarantee.

**TPM2 — hardware emulation verified**
- `swtpm 0.10.1-2` running on Arch host via unix socket
- `/dev/tpm0` and `/dev/tpmrm0` present in booted VM
- `systemd-analyze pcrs` confirms PCRs 0–10 and 14 carry real UEFI measurements
  (platform code, config, boot loader code, boot loader config, shim policy, etc.)

**Signed boot chain — shim + GRUB present on ESP**
- `/boot/efi/EFI/centos/shimx64.efi` → `grubx64.efi` (standard CentOS shim chain)
- Active EFI entry: `Boot0008 → \EFI\centos\shimx64.efi`
- bootupd-state.json present; bootupd manages the GRUB component

**UKI artifact — built and sections validated**
- `systemd-ukify`, `systemd-boot-unsigned`, `binutils` installed in image
- UKI built at image build time: 240MB, sections `.sbat .osrel .uname .linux .initrd` verified
- `/etc/kernel/install.conf`: `layout=uki`, `uki_generator=ukify`
- **The UKI exists in the container image filesystem layer only.**
  Inspection of the booted VM confirmed: no `/boot/efi/EFI/Linux/` directory,
  no UKI `.efi` on the ESP. `bootctl status` shows "Measured UKI: no".
  The UKI is an artifact, not an active boot component.

---

## 2. What's blocked and the exact root cause

**Full measured boot (PCRs 11–15 via systemd pcrphase) requires the UKI to be
the active boot entry under systemd-boot. It is not, and cannot be made so cleanly
on this platform.**

Root cause chain:

1. `bootc install to-filesystem` with `[install] bootloader = "systemd"` in
   `00-defaults.toml` fails: `"bootupd is required for ostree-based installs"`.
   The bootc-image-builder (Fedora 42-based) ships bootupd with only a
   `grub2-static` component. No systemd-boot component exists for CentOS Stream 10.
   bootupd cannot deploy systemd-boot because it has no registered component for it.

2. CentOS Stream 10 bootc is BLS/GRUB-centric. The ESP is owned and managed by
   bootc/ostree via kernel-install + BLS entries. bootc provides no sanctioned
   systemd-boot/UKI boot path on this platform.

3. A first-boot unit approach (run `bootctl install`, stage UKI to EFI/Linux/) would
   race bootc for ESP ownership. `bootc upgrade` calls kernel-install (BLS layout)
   and bootupd; either can overwrite or invalidate manually staged EFI entries.
   Failure mode: unbootable system after atomic update. This approach was deliberately
   rejected.

4. Therefore: PCRs 11–15 remain dormant. `systemd-pcrphase` services gate on
   `ConditionSecurity=measured-uki`, which is never satisfied. `bootctl status`
   shows "Measured UKI: no" indefinitely on this platform.

---

## 3. Options for a future "full measured boot" effort (not now)

**Switch base to Fedora bootc** — Fedora has stronger systemd-boot/UKI momentum
and bootupd ships the systemd-boot component. Rejected earlier in this project:
Fedora 42's GRUB causes a page-fault on Arch Linux's May 2026 edk2 OVMF. Would
reopen that problem. Not a drop-in replacement.

**Build on systemd-boot-native tooling from scratch** — Start from a minimal base
that uses systemd-boot as the primary bootloader (not GRUB+shim), configure bootupd
or bootctl from scratch without inheriting the CentOS ESP ownership model. This is
a separate deliberate project, not a patch on ArkaOS. Scope: Phase 5 or later.

---

## 4. Decision

Bank Phase 3 at current state.

The composefs rootfs integrity and TPM PCR 0–10 measurements are real and verified.
The PCR 11–15 gap is a platform limitation of CentOS Stream 10 bootc — it is not
a defect in ArkaOS. The NASM assembly EFI shim workaround was correctly avoided
throughout (would have been fragile, unmaintainable, and not the right abstraction).

Phase 4 (app sandboxing) is independent of the bootloader and is the next real work.

---

## Fallback images

| Artifact | Name |
|---|---|
| Container (GRUB, pre-Phase 3) | `localhost/arkaos:grub-fallback` |
| Container (Phase 3 milestone) | `localhost/arkaos:phase3-milestone` |
| qcow2 (bootable fallback) | `output/qcow2/disk-grub-fallback.qcow2` |
