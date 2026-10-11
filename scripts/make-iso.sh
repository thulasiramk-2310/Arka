#!/bin/bash
# make-iso.sh — build the ArkaOS installer ISO from localhost/arkaos:dev.
#
#   scripts/make-iso.sh OUTDIR
#
# Runs bootc-image-builder (anaconda-iso) inside the podman machine with
# installer/config.toml. The ISO lands in OUTDIR/bootiso/install.iso. Needs
# about 25 GB free inside the machine and room for the ISO on the host.
set -euo pipefail
OUT=${1:?usage: $0 OUTDIR}
out_abs=$(realpath -m "$OUT")
case "$out_abs" in
    "$HOME"/*) ;;
    *) echo "make-iso: OUTDIR must be under $HOME (shared into the podman machine)" >&2; exit 1 ;;
esac
mkdir -p "$out_abs"
repo=$(cd "$(dirname "$0")/.." && pwd)
podman machine ssh podman-machine-default \
    "podman run --rm --privileged --pull=never --security-opt label=type:unconfined_t \
       -v /var/lib/containers/storage:/var/lib/containers/storage \
       -v '/var$repo/installer/config.toml':/config.toml:ro \
       -v '/var$out_abs':/output \
       quay.io/centos-bootc/bootc-image-builder:latest \
       --type anaconda-iso --rootfs xfs localhost/arkaos:dev"
