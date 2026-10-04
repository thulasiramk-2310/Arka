#!/bin/bash
# make-disk.sh — install localhost/arkaos:dev into a raw disk image.
#
#   scripts/make-disk.sh OUT.raw [SIZE] [--serial]
#
# Runs `bootc install to-disk` inside the podman machine (it needs the image in
# the machine's root storage). The default image is what goes on real
# hardware: NO serial console. A forced console=ttyS0 makes systemd start a
# serial login on machines without a usable serial port, which crash-looped
# agetty on the Nokia (docs/NOKIA-USB-DIAGNOSIS.md). Pass --serial only for a
# VM debugging image.
set -euo pipefail

OUT=${1:?usage: $0 OUT.raw [SIZE] [--serial]}
SIZE=${2:-20G}
SERIAL=${3:-}
case "$SIZE" in --serial) SERIAL=--serial; SIZE=20G ;; esac

KARGS=()
if [ "$SERIAL" = --serial ]; then
    KARGS=(--karg console=tty0 --karg console=ttyS0)
fi

out_abs=$(realpath -m "$OUT")
case "$out_abs" in
    "$HOME"/*) ;;
    *) echo "make-disk: OUT must be under $HOME (shared into the podman machine)" >&2; exit 1 ;;
esac
dir=$(dirname "$out_abs"); file=$(basename "$out_abs")
mkdir -p "$dir"
rm -f -- "$out_abs"
truncate -s "$SIZE" "$out_abs"

# The machine mounts /home/Ram at /var/home/Ram.
mdir="/var$dir"
podman machine ssh podman-machine-default \
    "podman run --rm --privileged --pid=host --security-opt label=type:unconfined_t \
       -v /var/lib/containers:/var/lib/containers -v /dev:/dev -v '$mdir':/output \
       localhost/arkaos:dev bootc install to-disk --via-loopback --generic-image \
       --filesystem xfs ${KARGS[*]} '/output/$file'"
echo "make-disk: wrote $out_abs ($( [ -n "$SERIAL" ] && echo 'with' || echo 'without') serial console)"
