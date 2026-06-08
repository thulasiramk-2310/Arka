# ArkaOS

Privacy-first immutable desktop Linux. No unsandboxed apps, 60-second enforcement
loop, read-only rootfs. The GrapheneOS-for-desktop gap on a bootc image model.

Base: `centos-bootc:stream10`. Build: OCI container → bootc → qcow2.

---

## Architecture

```
┌─────────────────────────────────── ArkaOS ───────────────────────────────────┐
│                                                                               │
│  BOOT CHAIN                                                                   │
│  OVMF → GRUB → vmlinuz (6.12 LTS) → systemd                                 │
│                      │                                                        │
│            composefs overlay (rootfs read-only, ostree object store)         │
│            No path to overwrite system files at runtime.                     │
│                                                                               │
│  TPM2:  PCR 0-10 measured ✓  (firmware, bootloader, shim chain)             │
│         PCR 11-15 dormant  ✗  (requires UKI boot path — see Limitations)    │
│                                                                               │
├──────────────────────────────── arkad ───────────────────────────────────────┤
│                                                                               │
│  Rust daemon (static musl). Enforces on start, re-enforces every 60s.       │
│  Drift detection: if any enforcer's target state changes, it re-applies.    │
│                                                                               │
│  ┌─────────────────┐ ┌──────────────────────┐ ┌──────────┐ ┌─────────────┐ │
│  │    mac.rs       │ │       dns.rs          │ │hostname  │ │   ipv6.rs   │ │
│  │ WiFi+eth MAC    │ │ DNS-over-TLS          │ │   .rs    │ │use_tempaddr │ │
│  │ random per conn │ │ Quad9  9.9.9.9:853   │ │ arka     │ │     =2      │ │
│  │ NM conf.d       │ │ resolved.conf.d       │ │hostnamectl│ │  sysctl    │ │
│  └─────────────────┘ └──────────────────────┘ └──────────┘ └─────────────┘ │
│                                                                               │
├──────────────────────────── firefox sandbox ─────────────────────────────────┤
│                                                                               │
│  /usr/bin/firefox ──symlink──▶ /usr/bin/firefox-sandbox  (bwrap wrapper)    │
│                                /usr/bin/firefox-unwrapped (real ELF, hidden) │
│                                                                               │
│  Interception is baked at build time. composefs makes it read-only at       │
│  runtime — no path to replace the symlink or access the unwrapped binary.   │
│                                                                               │
│  Inside bwrap:                     │  Not visible to browser:               │
│  /usr /lib /bin /sbin  (ro bind)   │  ~/           (tmpfs — home hidden)    │
│  /etc                  (tmpfs)     │  /etc/NetworkManager/  (WiFi creds)    │
│    + resolv.conf, hosts, ssl,      │  /etc/arkad/           (daemon cfg)    │
│      pki, fonts, nsswitch,         │  /etc/machine-id       (host identity) │
│      ld.so.cache, alternatives     │  /run/dbus             (session bus)   │
│  /dev /proc            (dev/proc)  │  ~/.mozilla/    (discarded on exit)    │
│  /tmp /run /home /root (tmpfs)     │                                        │
│  ~/Downloads           (bind rw)   │  Network: host namespace.              │
│  --unshare-pid/ipc/uts             │  arkad DoT + IPv6 active below bwrap. │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

**Build pipeline:**
```
Arch host
 └─ podman machine ssh podman-machine-default
      ├─ podman build
      │    Stage 1: rust:alpine  →  arkad (x86_64-musl, static)
      │    Stage 2: centos-bootc:stream10
      │               arkad binary + systemd unit
      │               NM conf.d (MAC random)
      │               resolved.conf.d (DoT)
      │               UKI artifact (in image layer only — see Limitations)
      │               firefox → bwrap wrapper
      └─ bootc-image-builder  →  output/qcow2/disk.qcow2

qemu-system-x86_64 + swtpm  →  serial console :4445  (ram / arkaos)
```

---

## Prerequisites

- Arch Linux host (or similar)
- `podman` + `podman machine` (rootful, `podman-machine-default`)
- `qemu-system-x86_64`, `edk2-ovmf`
- `swtpm`, `swtpm_setup`
- podman machine VM disk: 20GB minimum (`podman machine init --disk-size 20`)

---

## Build

All build commands run inside the podman machine.
virtiofs mounts `/home/Ram` → `/var/home/Ram` inside the VM.

**1. Build the container image:**
```bash
podman machine ssh podman-machine-default \
  "podman build --pull=newer -t localhost/arkaos:dev /var/home/Ram/arkaos/"
```

**2. Produce the disk image:**

Kill any running boot-test VMs first (two QEMU instances + a build exhausts the VM's memory).

```bash
podman machine ssh podman-machine-default "podman run --rm --privileged \
  -v /var/lib/containers/storage:/var/lib/containers/storage \
  -v /var/home/Ram/arkaos/config.toml:/config.toml:ro \
  -v /var/home/Ram/arkaos/output:/output \
  quay.io/centos-bootc/bootc-image-builder:latest \
  --type qcow2 --rootfs xfs localhost/arkaos:dev"
```

Output: `output/qcow2/disk.qcow2`

**3. Initialize swtpm (once per tpm state directory):**
```bash
mkdir -p /tmp/arkaos-tpm
swtpm_setup --tpm2 --tpmstate /tmp/arkaos-tpm \
  --createek --decryption --create-ek-cert
```

**4. Boot:**
```bash
# First run: copy OVMF_VARS to a writable location
cp /usr/share/edk2/x64/OVMF_VARS.4m.fd ./OVMF_VARS.4m.fd

swtpm socket --tpmstate dir=/tmp/arkaos-tpm \
  --ctrl type=unixio,path=/tmp/arkaos-tpm.sock --tpm2 --daemon

qemu-system-x86_64 -enable-kvm -m 2048 -cpu host -smp 2 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
  -drive if=pflash,format=raw,file=OVMF_VARS.4m.fd \
  -drive file=output/qcow2/disk.qcow2,format=qcow2,if=virtio \
  -chardev socket,id=chrtpm,path=/tmp/arkaos-tpm.sock \
  -tpmdev emulator,id=tpm0,chardev=chrtpm \
  -device tpm-tis,tpmdev=tpm0 \
  -nographic \
  -serial telnet::4445,server,nowait \
  -monitor telnet::4444,server,nowait \
  -no-reboot

telnet localhost 4445   # login: ram / arkaos
```

---

## Demo: Sentinel-file Isolation Proof

The headline claim of Phase 4: a compromised browser process cannot read user
files, WiFi credentials, or daemon config. Run this in the VM serial console.

```bash
# Setup: write a sentinel to the real home directory
echo "topsecret" > ~/secret.txt

# 1. Sandbox cannot read the sentinel (real home is hidden behind tmpfs)
firefox --shell -c 'cat ~/secret.txt 2>&1'
# cat: /var/home/ram/secret.txt: No such file or directory

# 2. /home inside the sandbox is a tmpfs, not the real home
firefox --shell -c 'grep " /home " /proc/self/mountinfo'
# ... tmpfs on /home rw ...

# 3. WiFi credentials not reachable
firefox --shell -c 'ls /etc/NetworkManager 2>&1'
# ls: cannot access '/etc/NetworkManager': No such file or directory

# 4. arkad daemon config not reachable
firefox --shell -c 'ls /etc/arkad 2>&1'
# ls: cannot access '/etc/arkad': No such file or directory

# 5. /etc allowlist — nothing outside it
firefox --shell -c 'ls /etc'
# alternatives  fonts  hosts  ld.so.cache  ld.so.conf.d
# nsswitch.conf  pki  resolv.conf  ssl

# 6. Confirm the sentinel is still intact after sandbox exits
cat ~/secret.txt
# topsecret   (sandbox writes went to tmpfs and were discarded)

# Confirm /usr/bin/firefox IS the sandbox wrapper (no escape hatch)
ls -la /usr/bin/firefox
# /usr/bin/firefox -> firefox-sandbox
```

`firefox --shell` reuses the identical `BWRAP_ARGS` array from the wrapper. There
is no gap between what this test exercises and what ships.

---

## Limitations

### PCR 11-15: measured boot is incomplete

PCRs 0-10 are measured (firmware → GRUB → shim chain, verified with swtpm).
PCRs 11-15 (kernel + initrd + cmdline) are dormant.

`systemd-pcrphase` runs but hits `ConditionSecurity=measured-uki`, which requires
the kernel to be loaded via a signed UKI by systemd-boot. GRUB loads the kernel
directly from BLS entries — PCR 11 is never extended.

The UKI artifact exists in the image (`/usr/lib/modules/<kver>/<kver>.efi`, 240MB,
sections `.sbat .osrel .uname .linux .initrd` verified). It is never staged to the
ESP because `bootupd-0.2.31` — the version in both the CentOS image and the
bootc-image-builder container — has only a `grub2-static` component.
No `sdboot` component = no path to get the UKI onto the ESP via `[install]
bootloader = "systemd"`.

**What this means in practice:** No TPM-sealed disk encryption, no remote
attestation against kernel/initrd state. The signed, immutable composefs rootfs
is the integrity story. PCR sealing requires a future bootupd that ships the
sdboot component.

**We tested the Fedora path.** Branch `research/fedora-systemd-boot` ported the
full build to `fedora-bootc:42` and set `bootloader = "systemd"`. **Result: same
failure.** Fedora 42 ships `bootupd-0.2.31-1.fc42` with only `grub2-static` —
identical to CentOS. The hypothesis that Fedora's bootupd would differ was wrong
for this version. See `RESEARCH.md` on that branch for the full finding, root
cause, and forward paths.

### Sandbox scope

- Network runs in the host namespace (browser needs internet). arkad's DNS-over-TLS
  and IPv6 privacy extensions operate at the kernel/NM layer below bwrap — they
  apply to all browser traffic regardless.
- Browser profile (`.mozilla/`) lives in `/home` tmpfs and is discarded on process
  exit. No persistent session state on disk. Intentional.
- Wayland/X11 socket isolation not tested (headless-only build).
- Only Firefox is sandboxed. Other desktop apps run unsandboxed. Phase 4 scoped
  to one reference app.

---

## Files

```
Containerfile          multi-stage build: rust:alpine → centos-bootc:stream10
arkad/
  src/main.rs          main loop: enforce all, sleep 60s, re-enforce on drift
  src/config.rs        serde config (secure defaults, works with no file)
  src/enforcers/       mac.rs  dns.rs  hostname.rs  ipv6.rs
  arkad.service        systemd unit
  arkad.toml           /etc/arkad/arkad.toml defaults
arkaos-firefox         bwrap wrapper — IS /usr/bin/firefox in the deployed image
config.toml            bootc-image-builder config (qcow2, XFS rootfs)
OVMF_VARS.4m.fd        writable NVRAM copy (gitignored, created on first boot)
PHASE3-FINDINGS.md     measured-boot investigation: what's live, what's blocked
PHASE4-SANDBOX.md      sandbox model, isolation proof, scope boundaries
RESEARCH.md            Fedora systemd-boot experiment: FAIL + root cause
  (on branch research/fedora-systemd-boot)
```
