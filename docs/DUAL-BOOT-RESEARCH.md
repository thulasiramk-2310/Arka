# Dual boot: how other systems handle it (research, 2026-11-16)

Web research only, done by a Sonnet agent. Nothing here has been tested on ArkaOS.
The official sources are strong for Ubuntu, Fedora, Bazzite and Microsoft. The sources for
Pop!_OS, Mint, openSUSE/Agama, NixOS/Calamares and Intel Macs are thin or community-only.
No source documents how bootc/bootupd behaves next to Windows.

## What each system does

| System | Dual-boot offer | Resizes BitLocker | Boot menu / finding Windows | ESP | BitLocker advice |
|---|---|---|---|---|---|
| Ubuntu | alongside, erase, manual | no; shows "turn off BitLocker" (sometimes wrongly) | GRUB; os-prober off by default since GRUB 2.06 | shares the Windows ESP | turn it off |
| Fedora | WebUI free-space option, manual | no (RFE open) | GRUB; os-prober was needed for Windows | shares the Windows ESP (~21 MB used) | issue #278: Windows booted via GRUB asks for the recovery key |
| Arch | manual | not covered | systemd-boot and rEFInd find Windows on their own; GRUB needs a chainload entry | reuse | turn off Fast Startup; UTC on both |
| NixOS | Calamares, manual | not found | systemd-boot finds Windows | the 100 MB Windows ESP is too small; grow it or use XBOOTLDR | not found |
| Pop!_OS | custom install | not found | systemd-boot; Windows entry often missing | own ESP | not found |
| Mint | alongside (sometimes not offered) | no | GRUB | reuse | turn off, back up the key |
| openSUSE | Agama "do not modify" | not found | GRUB + os-prober; one bug: rollback dropped the Windows entries | not found | suspend, save the key |
| Silverblue / Kinoite | Anaconda | no | static grub.cfg; Windows added by hand in `/boot/grub2/user.cfg` | separate ESP reported to work; one install formatted the Windows ESP | same as Fedora |
| Bazzite | separate drive preferred | no; turn it off first | GRUB; `ujust regenerate-grub` adds Windows | 300 MB ESP; each atomic OS gets its own | turn off BitLocker and Fast Startup |

## What most of them agree on

1. No Linux installer resizes a BitLocker volume. Shrink from Windows first, install into the free space.
2. Save the BitLocker recovery key first and expect a recovery prompt after a boot-chain change.
3. Turn off Fast Startup and hibernation in Windows.
4. A separate drive is the safest setup.
5. Never format an existing ESP. Make sure the shared one has room, or give Linux its own.
6. Don't rely on os-prober. Add the Windows entry in a fixed, repeatable way (or use the firmware boot menu).
7. Use UTC on both systems, or set `RealTimeIsUniversal` in Windows.
8. Windows updates reorder the firmware boot entries and sometimes remove the Linux one.

## What this means for ArkaOS (Anaconda, bootc, LUKS by default, GRUB through bootupd)

Installer:
- Detect Windows and BitLocker. Say plainly that ArkaOS cannot resize BitLocker, that the user should shrink
  from Windows first and save the recovery key, and that Windows may ask for that key once afterwards.
- Warn about Fast Startup and hibernation.
- Say the Windows menu entry is not guaranteed, and give the firmware boot-menu key as the fallback.
- Recommend a separate drive when there is one. Keep dual boot labelled untested until the test matrix passes.

Decisions:
- Add Windows as a fixed chainload entry (the Kinoite way) instead of os-prober; on ArkaOS in `custom.cfg`
  (see the bootupd section below). Check that it survives
  `bootc upgrade` and a rollback.
- Find out what bootupd does to a shared ESP before shipping. Fedora's own change page warns that it overwrites
  files in `/boot/efi`.
- Consider a "restart into Windows" helper using `efibootmgr --bootnext`, the direction Fedora preferred.

Tests, in a VM (OVMF + swtpm) and then on real hardware:
- Windows 11 with BitLocker on (shrunk from Windows) and off; Windows 10.
- Secure Boot on and off.
- Shared 100 MB ESP versus a separate ESP.
- After each install: ArkaOS boots, Windows boots from GRUB, and Windows boots from the firmware menu.
- After a Windows cumulative update: the boot order and `\EFI\fedora` are still there.
- After `bootc upgrade` and a rollback: the Windows entry is still there.
- LUKS unlock still works after booting Windows.
- Real hardware: count the BitLocker recovery prompts on a TPM laptop that came with device encryption.

## What bootupd does to a shared ESP (read from its source, 2026-11-18)

From coreos/bootupd main at c9808ed, by reading the code; nothing here was run.

- Install copies its whole payload onto the ESP: `EFI/fedora/*` and the fallback `EFI/BOOT/BOOTX64.EFI`
  (`Efi::install` in `src/efi.rs`). So it replaces another system's fallback loader. It never names
  `EFI/Microsoft`, so Windows Boot Manager's own files are left alone.
- An update only changes files in the tree recorded in `/boot/bootupd-state.json` and only deletes files that
  were in that record (`Efi::run_update`, `src/filetree.rs`). Other systems' folders are never deleted.
- Firmware boot entries (`efibootmgr`) are written only at install, and only with `--update-firmware`; an update
  writes none. Whether Anaconda passes that flag is not checked yet.
- `bootupctl update` does not rewrite `grub.cfg`. The static GRUB config reads `user.cfg` before the ArkaOS
  entries (meant for a GRUB password) and `custom.cfg` after them (`src/grub2/configs.d/41_custom.cfg`), and
  bootupd writes neither. So the Windows entry goes in `custom.cfg`, not `user.cfg`. That hook exists only when
  the static configs are installed; whether Fedora's bootc install uses them is to be checked on an installed
  system.
- bootupd has no os-prober and no Windows entry of its own.

The installer (`installer/config.toml`, `%post`) now writes a `custom.cfg` that searches every disk for
`/EFI/Microsoft/Boot/bootmgfw.efi` at each start and shows "Windows" only when it is found.

## VM results, 2026-11-18 (QEMU + OVMF, a Windows-shaped disk, no real Windows)

- `bootc install to-filesystem` does install the static GRUB configs; the `custom.cfg` Windows entry shows in
  the menu, and only on a disk that has `bootmgfw.efi`.
- bootc writes a firmware entry at install, named after the image's `/etc/system-release`, and first deletes
  every entry with that name. With Fedora's file the name is "Fedora", so it would remove a real Fedora's entry.
  The Containerfile now writes "ArkaOS release 44"; until that image is built, the installer refuses when an
  entry with the image's name already exists.
- bootc leaves `/boot` and the root filesystem read-only after install; the installer mounts them again on
  their own to add `custom.cfg` and the install log.
- `arka-install` (systemd-repart + bootc): three installs (two next to the Windows layout, one on an empty disk).
  Windows' four partitions were byte-for-byte unchanged, `bootmgfw.efi` kept its hash, LUKS unlock and the
  desktop worked, and `bootc upgrade --check` saw the published image. A fresh install uses 11 to 12 GB.
- Not tested: real Windows 11, BitLocker on a TPM, Secure Boot, real hardware. Dual boot stays untested.

## Sources

- Ubuntu, BitLocker during installation: https://ubuntu.com/desktop/docs/en/latest/reference/bitlocker-during-ubuntu-installation
- Ubuntu bugs: https://bugs.launchpad.net/bugs/2068915, https://bugs.launchpad.net/subiquity/+bug/2045480
- GRUB 2.06 os-prober default: https://www.omgubuntu.co.uk/2021/12/grub-doesnt-detect-windows-linux-distros-fix/amp
- SBAT update of August 2024: https://www.bleepingcomputer.com/news/microsoft/august-windows-security-update-breaks-dual-boot-on-linux-systems/, Ubuntu fix https://lists.snapcraft.io/archives/foundations-bugs/2024-September/523293.html
- Anaconda WebUI steps: https://anaconda-installer.readthedocs.io/en/latest/anaconda-webui/docs/installation-steps.html
- Fedora BitLocker resize RFE: https://bugzilla.redhat.com/show_bug.cgi?id=2067248
- Fedora Workstation issue #278: https://pagure.io/fedora-workstation/issue/278
- Fedora resize-risk bug: https://bugzilla.redhat.com/show_bug.cgi?id=1134102
- Kinoite dual boot thread: https://discussion.fedoraproject.org/t/dual-boot-fedora-kinoite-windows-working-but-the-solution-feels-brittle/203065
- Silverblue formatted the ESP: https://discussion.fedoraproject.org/t/silverblue-32-formatted-efi-partition-during-install-cannot-boot-windows-fixed/19249
- Fedora entry gone after a Windows update: https://discussion.fedoraproject.org/t/fedora-grub-disappeared-from-dual-boot-after-windows-11-update/127803?page=2
- bootupd change page: https://fedoraproject.org/wiki/Changes/BootLoaderUpdatesPhase1, ESP lookup bug https://bugzilla.redhat.com/show_bug.cgi?id=2304328
- Bazzite dual boot guide: https://docs.bazzite.gg/General/Installation_Guide/dual_boot_setup_guide/
- NixOS: https://nixos.wiki/wiki/Dual_Booting_NixOS_and_Windows
- openSUSE rollback bug: https://lists.opensuse.org/archives/list/bugs@lists.opensuse.org/thread/PL2O6UVK2HLXF7U3O24T4MYVTWNXT3GV
- Microsoft, BitLocker recovery triggers: https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/countermeasures
- Arch wiki, Dual boot with Windows (read through a mirror; may be out of date)
