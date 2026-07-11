# Building ArkaOS

From source tree to bootable qcow2 in three commands. Everything runs inside
the rootful podman machine (`podman-machine-default`), which mounts your home
at `/var/home/<you>` via virtiofs.

## Prerequisites (one-time)

- Arch/any host with podman + qemu + swtpm
- `podman machine init --rootful --memory 6144 --disk-size 40 podman-machine-default`
  (40 GB minimum — the KDE image alone is ~10 GB in storage)
- Fedora's OVMF firmware (Fedora 42 GRUB crashes Arch's edk2 — see
  "Solved issues" in CLAUDE.md for the extraction one-liner producing
  `OVMF_CODE_4M_f42.qcow2` + `OVMF_VARS_4M_f42.qcow2`)

## 1. Build the OS image

```sh
podman machine ssh podman-machine-default \
  "cd /var/home/$USER/arkaos && podman build -t localhost/arkaos:dev ."
```

~15 min warm cache; hours cold (the `@kde-desktop-environment` layer downloads
~2.5 GB of RPMs — `--setopt=retries=25` in the Containerfile rides out mirror
flakes; if the layer still fails it is almost always transient: rerun).

## 2. Build the disk image (bootc-image-builder)

```sh
podman machine ssh podman-machine-default \
  "cd /var/home/$USER/arkaos && rm -rf output/qcow2 && podman run --rm --privileged \
    -v /var/lib/containers/storage:/var/lib/containers/storage \
    -v /var/home/$USER/arkaos/config.toml:/config.toml:ro \
    -v /var/home/$USER/arkaos/output:/output \
    quay.io/centos-bootc/bootc-image-builder:latest \
    --type qcow2 --rootfs xfs localhost/arkaos:dev"
```

Output: `output/qcow2/disk.qcow2`.

## 3. Boot it

```sh
cp OVMF_VARS_4M_f42.qcow2 OVMF_VARS_4M_f42_boot.qcow2
qemu-system-x86_64 -enable-kvm -m 6144 -cpu host -smp 2 -machine q35 \
  -drive if=pflash,format=qcow2,readonly=on,file=OVMF_CODE_4M_f42.qcow2 \
  -drive if=pflash,format=qcow2,file=OVMF_VARS_4M_f42_boot.qcow2 \
  -drive file=output/qcow2/disk.qcow2,format=qcow2,if=virtio \
  -device virtio-vga -display gtk -usb -device usb-tablet \
  -serial telnet::4445,server,nowait -monitor telnet::4444,server,nowait
```

- Serial console: `telnet localhost 4445`
- QEMU monitor (screendump, sendkey): `telnet localhost 4444`
- TPM (optional, for PCR work): see CLAUDE.md §3c for the swtpm invocation.

## Hard-won rules — break these and lose hours

1. **Never prune between build and BIB.** `podman builder prune -af` deletes
   cache layers that back the tagged image; the tag dies. Clean disk BEFORE
   building if needed.
2. **Killed builds corrupt storage bookkeeping.** If `podman system df` says
   12 GB but `du /var/lib/containers/storage/overlay` says 23 GB, the orphans
   are invisible to prune — `podman system reset -f` inside the machine is the
   only fix.
3. **`-device virtio-vga`, not virtio-gpu** — wlroots/KWin leave outputs
   inactive on virtio-gpu.
4. **Kill boot-test VMs before running BIB** — two QEMU VMs + osbuild exceed
   machine memory and the machine OOM-dies mid-build.
5. **A `podman machine ssh` build survives your terminal dying.** Before
   assuming a build is lost, `pgrep -f 'podman build'` inside the machine.
6. **`--exclude=rootfiles`** stays on the KDE dnf line (its /root dotfiles
   collide with the bootc base and fail the whole transaction).

## Iterating cheaply

- Rust-only change (arkad, shell apps): the builder stages rebuild but the KDE
  layer stays cached — keep branding/config edits BELOW the
  `@kde-desktop-environment` RUN in the Containerfile to preserve that cache.
- Theme/branding-only change: seconds of rebuild + BIB (~8 min).
- Host-side `cargo check` works for non-layer-shell crates (dashboard, etc.);
  crates using gtk4-layer-shell need the container (host lacks the dev lib).
