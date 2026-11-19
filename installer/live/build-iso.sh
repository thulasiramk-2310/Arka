#!/bin/bash
# Makes the ArkaOS live ISO from the live image mounted read-only at /rootfs, into
# /output. Runs in a privileged Fedora container (see .github/workflows/live-iso.yml).
#
# Adapted from ublue-os/titanoboa build_iso.sh (Apache-2.0): a squashfs of the image,
# its kernel and initramfs, and Fedora's signed shim + GRUB on a FAT partition appended
# to the ISO, so it starts from a USB stick or a DVD, with Secure Boot on or off.
# Differences: ownership is kept (no -all-root: the setup account's home must stay its
# own), the boot menu is fixed here, and the ESP holds only shim, GRUB and MokManager
# (no fallback fbx64.efi, which would add a firmware boot entry for the stick).
set -euxo pipefail

LABEL=ARKAOS_LIVE
ARGS="root=live:CDLABEL=$LABEL rd.live.image rd.live.overlay.overlayfs=1 enforcing=0 quiet rhgb"
W=/work

dnf install -y -q squashfs-tools xorriso mtools dosfstools
rm -rf "$W/iso" "$W/esp" "$W/esp.img"
mkdir -p "$W/iso/LiveOS" "$W/iso/images/pxeboot" "$W/esp/EFI/BOOT" "$W/esp/EFI/fedora"

# The system. /sysroot and /ostree belong to installed systems.
mksquashfs /rootfs "$W/iso/LiveOS/squashfs.img" -noappend -e sysroot -e ostree \
  -comp zstd -Xcompression-level 15

cp /rootfs/usr/lib/modules/*/vmlinuz "$W/iso/images/pxeboot/vmlinuz"
cp /rootfs/usr/lib/modules/*/initramfs.img "$W/iso/images/pxeboot/initrd.img"

# Fedora's signed boot files, from wherever this image keeps them.
find_efi() {
  find /rootfs/usr/lib/efi /rootfs/usr/lib/bootupd /rootfs/boot/efi -name "$1" -print -quit 2>/dev/null
}
shim=$(find_efi shimx64.efi); grub=$(find_efi grubx64.efi); mm=$(find_efi mmx64.efi)
for f in "$shim" "$grub" "$mm"; do [ -n "$f" ] || { echo "missing an EFI boot file" >&2; exit 1; }; done

cat > "$W/grub.cfg" <<CFG
set timeout=5
set default=0
set menu_auto_hide=false
insmod all_video
set gfxpayload=keep
insmod part_gpt
insmod iso9660
search --no-floppy --set=root -l '$LABEL'

menuentry 'Install ArkaOS' --class fedora {
  linux /images/pxeboot/vmlinuz $ARGS
  initrd /images/pxeboot/initrd.img
}
menuentry 'Install ArkaOS (basic graphics)' --class fedora {
  linux /images/pxeboot/vmlinuz $ARGS nomodeset
  initrd /images/pxeboot/initrd.img
}
CFG

# Shim starts as BOOTX64.EFI and loads grubx64.efi beside it; Fedora's GRUB reads its
# grub.cfg from /EFI/fedora, so both folders get the same files.
for d in BOOT fedora; do
  cp "$grub" "$mm" "$W/grub.cfg" "$W/esp/EFI/$d/"
done
cp "$shim" "$W/esp/EFI/BOOT/BOOTX64.EFI"
cp "$shim" "$W/esp/EFI/fedora/shimx64.efi"
cp -r "$W/esp/EFI" "$W/iso/EFI"

# 64 MiB with 512-byte clusters is a valid FAT32 (more than 65525 clusters).
truncate -s 64M "$W/esp.img"
mkfs.fat -F 32 -s 1 -n ARKA_EFI "$W/esp.img"
mcopy -s -i "$W/esp.img" "$W/esp/EFI" ::

xorriso -as mkisofs \
  -R -V "$LABEL" \
  -partition_offset 16 \
  -appended_part_as_gpt \
  -append_partition 2 C12A7328-F81F-11D2-BA4B-00A0C93EC93B "$W/esp.img" \
  -iso_mbr_part_type EBD0A0A2-B9E5-4433-87C0-68B6B72699C7 \
  -e --interval:appended_partition_2:all:: \
  -no-emul-boot \
  -iso-level 3 \
  -o /output/arkaos-live.iso \
  "$W/iso"
cd /output && sha256sum arkaos-live.iso > arkaos-live.iso.sha256
ls -la /output
