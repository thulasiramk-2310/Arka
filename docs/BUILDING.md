# Building ArkaOS

The recipe now targets Fedora 44 as an unvalidated candidate. Complete the
[release and hardware gates](RELEASE-VALIDATION.md) before distributing an image.
Build and VM commands below operate on disposable guest images. Never attach
Windows partitions, internal host disks, host EFI storage or host firmware
variables. The separate USB procedure is for an explicitly selected disposable
external test stick only.

Everything runs inside
the rootful podman machine (`podman-machine-default`), which mounts your home
at `/var/home/<you>` via virtiofs.

## Prerequisites (one-time)

- Arch/any host with podman + qemu + swtpm
- `podman machine init --rootful --memory 6144 --disk-size 40 podman-machine-default`
  (40 GB minimum — the KDE image alone is ~10 GB in storage)
- Matching Fedora OVMF firmware in disposable files. Extract it once:
  ```
  for f in CODE VARS; do
    podman machine ssh podman-machine-default \
      "podman run --rm docker.io/fedora:44 bash -c \
      'dnf install -y edk2-ovmf -q &>/dev/null; base64 /usr/share/edk2/ovmf/OVMF_${f}_4M.qcow2'" \
      | base64 -d > OVMF_${f}_4M_f44.qcow2
  done
  ```

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
cp OVMF_VARS_4M_f44.qcow2 OVMF_VARS_4M_f44_boot.qcow2
qemu-system-x86_64 -enable-kvm -m 6144 -cpu host -smp 2 -machine q35 \
  -drive if=pflash,format=qcow2,readonly=on,file=OVMF_CODE_4M_f44.qcow2 \
  -drive if=pflash,format=qcow2,file=OVMF_VARS_4M_f44_boot.qcow2 \
  -drive file=output/qcow2/disk.qcow2,format=qcow2,if=virtio \
  -device virtio-vga -display gtk -usb -device usb-tablet \
  -serial telnet:127.0.0.1:4445,server,nowait -monitor telnet:127.0.0.1:4444,server,nowait
```

- Serial console: `telnet localhost 4445`
- QEMU monitor (screendump, sendkey): `telnet localhost 4444`
- TPM (optional, for PCR work):
  ```
  swtpm_setup --tpm2 --tpmstate /tmp/arkaos-tpm --createek --decryption --create-ek-cert
  swtpm socket --tpmstate dir=/tmp/arkaos-tpm --ctrl type=unixio,path=/tmp/arkaos-tpm.sock --tpm2 --daemon
  # then add to the QEMU command:
  #   -chardev socket,id=chrtpm,path=/tmp/arkaos-tpm.sock -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0
  ```

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

## Writing a raw image to a disposable USB stick

Use a raw image, not a qcow2 file. Identify the external test stick by its model,
serial number and capacity before replacing `/dev/sdX` below. The target is the
whole USB device, not a partition. These commands erase that selected stick;
never select the internal Linux/Windows disk. Unmount the stick's partitions
before writing. This procedure is documentation, not a first-boot action.

```sh
sudo dd if=arkaos-preview.raw of=/dev/sdX bs=4M status=progress conv=fsync &&
  sudo sgdisk -e /dev/sdX &&
  sudo sgdisk -v /dev/sdX &&
  sync
```

When the stick is larger than the image, the backup GPT remains at the image's
old end. `sgdisk -e` relocates it to the device's end; `-v` checks the resulting
partition metadata. It does not resize partitions or repair every kind of GPT
corruption. Stop on errors. The Nokia field notes reported a firmware USB boot
entry missing until this relocation was performed; this is separate from a
kernel or desktop crash. See the [sgdisk manual](https://www.rodsbooks.com/gdisk/sgdisk.html).
Do not run automatic GPT repair on first boot: that could target an internal
Linux or Windows drive.

### Existing sticks with damaged GPT metadata

The Nokia extraction on 2026-10-05 found a **primary partition-table CRC
failure with a valid backup**, not just a backup header at an old image boundary.
See [the diagnosis](NOKIA-USB-DIAGNOSIS.md). Do not treat `sgdisk -e` as a generic
repair command for such a stick or reflash over evidence you still need.
Verification output must be reviewed for reported problems; a command finishing
is not proof of an intact table. Preserve needed files before planning recovery.
