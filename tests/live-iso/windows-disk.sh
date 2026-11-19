#!/bin/bash
# windows-disk.sh FILE: an 80 GiB sparse disk laid out like a Windows 11 install (EFI
# system partition with EFI/Microsoft/Boot/bootmgfw.efi, Microsoft reserved, 24 GiB
# data, 1 GiB recovery at the end) with about 54 GiB free between data and recovery.
# The data partition gets 17 random 1 MiB blocks; FILE.sums records their hashes and
# the boot file's, for check-windows.sh to compare after an install.
set -euo pipefail
f=$1
rm -f "$f"; truncate -s 80G "$f"
sgdisk -n 1:2048:+260M -t 1:EF00 -c 1:"EFI System Partition" \
       -n 2:0:+16M -t 2:0C01 -c 2:"Microsoft reserved" \
       -n 3:0:+24G -t 3:0700 -c 3:"Basic data partition" \
       -n 4:-1G:0 -t 4:2700 -c 4:"Windows recovery" "$f" >/dev/null
# FAT32 needs at least 65525 clusters: 260 MiB with 512-byte clusters has enough.
mkfs.fat --offset 2048 -F 32 -s 1 -n SYSTEM "$f" 266240 >/dev/null
o=$((2048 * 512))
head -c 1500000 /dev/urandom > "$f.bootmgfw"
mmd -i "$f@@$o" ::EFI ::EFI/Microsoft ::EFI/Microsoft/Boot
mcopy -i "$f@@$o" "$f.bootmgfw" ::EFI/Microsoft/Boot/bootmgfw.efi
start3=$(sgdisk -i 3 "$f" | awk '/First sector/ {print $3}')
: > "$f.sums"
for i in $(seq 0 16); do
  off=$(( start3 * 512 + i * 1536 * 1024 * 1024 ))
  head -c 1M /dev/urandom | dd of="$f" bs=1M seek=$((off / 1048576)) conv=notrunc status=none
  echo "$off $(dd if="$f" bs=1M skip=$((off / 1048576)) count=1 status=none | sha256sum | cut -c1-64)" >> "$f.sums"
done
echo "bootmgfw $(sha256sum < "$f.bootmgfw" | cut -c1-64)" >> "$f.sums"
sgdisk -p "$f" | tail -5
