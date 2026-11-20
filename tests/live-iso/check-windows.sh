#!/bin/bash
# check-windows.sh FILE GPT-BEFORE: after an install, the four Windows partitions have
# the same start, end, type and GUID, the data blocks and the boot file are unchanged.
set -euo pipefail
f=$1; before=$2; bad=0
for p in 1 2 3 4; do
  if [ "$(sgdisk -i $p "$f")" != "$(sed -n "/^== $p\$/,/^== /p" "$before" | sed '1d;$d')" ]; then
    echo "partition $p CHANGED"; bad=1
  fi
done
while read -r off sum; do
  [ "$off" = bootmgfw ] && continue
  [ "$(dd if="$f" bs=1M skip=$((off / 1048576)) count=1 status=none | sha256sum | cut -c1-64)" = "$sum" ] \
    || { echo "data block at $off CHANGED"; bad=1; }
done < "$f.sums"
o=$((2048 * 512))
got=$(mtype -i "$f@@$o" ::EFI/Microsoft/Boot/bootmgfw.efi | sha256sum | cut -c1-64)
[ "$got" = "$(awk '/^bootmgfw/ {print $2}' "$f.sums")" ] || { echo "bootmgfw.efi CHANGED"; bad=1; }
# The fallback loader is reported, not required: the installer says on screen whether it changed.
fb=$(mtype -i "$f@@$o" ::EFI/Boot/bootx64.efi 2>/dev/null | sha256sum | cut -c1-64)
if [ "$fb" = "$(awk '/^bootmgfw/ {print $2}' "$f.sums")" ]; then echo "fallback EFI/Boot/bootx64.efi: unchanged (Windows')"
else echo "fallback EFI/Boot/bootx64.efi: REPLACED"; mdir -i "$f@@$o" ::EFI/Boot || true; fi
[ $bad = 0 ] && echo "Windows partitions, 16 data blocks and bootmgfw.efi unchanged"
exit $bad
