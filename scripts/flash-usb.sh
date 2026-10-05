#!/bin/bash
# flash-usb.sh — write an ArkaOS raw disk image to a USB stick, verifiably.
#
#   sudo scripts/flash-usb.sh IMAGE.raw /dev/sdX
#
# Production procedure (see docs/NOKIA-USB-DIAGNOSIS.md for why each step exists):
#   1. refuse anything that is not a whole, removable, USB-attached disk
#      (internal NVMe/SATA disks are refused; verify the target identity);
#   2. wipe BOTH GPT copies and old signatures, so a stale backup table from a
#      previous image can't be mistaken for the real one;
#   3. write with direct I/O and fsync;
#   4. read the written bytes back and compare their SHA-256 with the image;
#   5. move the backup GPT to the true end of the stick (`sgdisk -e`) and
#      require `sgdisk -v` to report no problems.
# Any failed check stops the script with a non-zero exit.
set -euo pipefail

IMG=${1:?usage: sudo $0 IMAGE.raw /dev/sdX}
DEV=${2:?usage: sudo $0 IMAGE.raw /dev/sdX}

die() { echo "flash-usb: $*" >&2; exit 1; }
[ "$(id -u)" = 0 ] || die "run with sudo"
for t in lsblk wipefs sgdisk dd sha256sum partprobe blockdev realpath stat; do
    command -v "$t" >/dev/null || die "missing tool: $t"
done
IMG=$(realpath -e -- "$IMG") || die "cannot resolve image"
if [ ! -f "$IMG" ] || [ ! -r "$IMG" ] || [ ! -s "$IMG" ]; then
    die "image must be a readable, nonempty regular file: $IMG"
fi
# Validate and hash the source before any destructive operation.
export LC_ALL=C
[ "$(dd if="$IMG" bs=1 skip=512 count=8 status=none)" = 'EFI PART' ] || die "image has no raw GPT header"
source_verify=$(sgdisk -v "$IMG" 2>&1) || die "cannot validate source GPT"
[[ "$source_verify" == *"No problems found"* ]] || die "source GPT is damaged"
img_sha=$(sha256sum "$IMG" | cut -d' ' -f1)
[ -b "$DEV" ] || die "not a block device: $DEV"

# --- 1. target must be a whole removable USB disk --------------------------------
name=$(basename "$DEV")
[ "$(lsblk -dno TYPE "$DEV")" = disk ] || die "$DEV is not a whole disk (give /dev/sdX, not a partition)"
[ "$(lsblk -dno TRAN "$DEV")" = usb ] || die "$DEV is not USB-attached — refusing"
[ "$(cat "/sys/block/$name/removable" 2>/dev/null)" = 1 ] ||
    [ "${ALLOW_FIXED_USB:-}" = 1 ] || die "$DEV is not marked removable (set ALLOW_FIXED_USB=1 only for a USB SSD you are sure about)"
root_disk=$(lsblk -no PKNAME "$(findmnt -no SOURCE /)" 2>/dev/null || true)
[ "$name" != "$root_disk" ] || die "$DEV holds the running system — refusing"
if lsblk -no MOUNTPOINTS "$DEV" | grep -q .; then
    die "$DEV has mounted partitions — unmount them first"
fi
img_bytes=$(stat -c %s "$IMG")
dev_bytes=$(blockdev --getsize64 "$DEV")
[ "$dev_bytes" -ge "$img_bytes" ] || die "stick ($dev_bytes bytes) is smaller than the image ($img_bytes bytes)"

echo "Target : $DEV  $(lsblk -dno VENDOR,MODEL,SIZE "$DEV" | tr -s ' ')"
echo "Image  : $IMG  ($((img_bytes / 1024 / 1024)) MiB)"
lsblk -o NAME,SIZE,FSTYPE,LABEL "$DEV"
read -r -p "Everything on $DEV will be erased. Type the device name ($name) to continue: " ok
[ "$ok" = "$name" ] || die "not confirmed"

# --- 2. wipe old partition tables (both GPT copies) and signatures ---------------
echo "== wiping old partition tables and signatures"
wipefs --all --force "$DEV" >/dev/null
sgdisk --zap-all "$DEV" >/dev/null
partprobe "$DEV" 2>/dev/null || true

# --- 3. write -------------------------------------------------------------------
echo "== writing image"
dd if="$IMG" of="$DEV" bs=4M oflag=direct conv=fsync status=progress
sync

# --- 4. read back and verify ------------------------------------------------------
echo "== verifying written data"
echo 3 > /proc/sys/vm/drop_caches
dev_sha=$(head -c "$img_bytes" "$DEV" | sha256sum | cut -d' ' -f1)
[ "$dev_sha" = "$img_sha" ] || die "verification FAILED: stick content differs from the image (bad stick or write error)"
echo "   sha256 matches: $img_sha"

# --- 5. fix and verify the GPT for this stick's real size -------------------------
echo "== relocating backup GPT to the end of the stick"
sgdisk -e "$DEV" >/dev/null
partprobe "$DEV" 2>/dev/null || true
verify=$(sgdisk -v "$DEV" 2>&1)
echo "$verify" | grep -q "No problems found" || { echo "$verify" >&2; die "GPT verification FAILED"; }
echo "   sgdisk -v: no problems found"
sync

lsblk -o NAME,SIZE,FSTYPE,LABEL,PARTLABEL "$DEV"
echo "Done. $DEV is ready to boot."
