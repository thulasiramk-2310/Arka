# ArkaOS

Privacy-first immutable Linux desktop. CentOS Stream 10 base, bootc image model.
The GrapheneOS-for-desktop gap: hardened defaults, isolation-by-default, no unsandboxed apps.

```
┌──────────────────────────────────────────────────────────────────┐
│                        ArkaOS Image                              │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  Phase 4 — App Sandboxing                                  │  │
│  │  /usr/bin/firefox → bwrap wrapper                         │  │
│  │  tmpfs /home  ·  /etc allowlist  ·  --unshare-pid/ipc/uts │  │
│  ├────────────────────────────────────────────────────────────┤  │
│  │  Phase 2 — arkad privacy daemon (Rust, musl, 60s watch)   │  │
│  │  MAC random  ·  DoT Quad9  ·  hostname=arka  ·  IPv6 EUI  │  │
│  ├────────────────────────────────────────────────────────────┤  │
│  │  Phase 3 — Integrity                                       │  │
│  │  composefs read-only rootfs  ·  TPM2 PCR 0-10 measured    │  │
│  │  UKI artifact in image layer (PCRs 11-15: see Limitations) │  │
│  ├────────────────────────────────────────────────────────────┤  │
│  │  Phase 1 — Base                                            │  │
│  │  centos-bootc:stream10  ·  bootc immutable OCI model       │  │
│  │  NM MAC random (conf.d)  ·  XFS root via 00-defaults.toml  │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
         │                                    │
   podman build                    bootc-image-builder
   (inside podman machine)         → qcow2/disk image
```

---

## Prerequisites

- Arch Linux host (or similar; adjust paths for your distro)
- `podman` + `podman machine` with a running `podman-machine-default` VM
- `qemu-system-x86_64`, `edk2-ovmf`
- `swtpm` + `swtpm_setup` (for TPM tests)

---

## Build

All build commands run inside `podman-machine-default` (virtiofs mounts `/home/Ram` at `/var/home/Ram`).

**1. Build the container image:**
```bash
podman machine ssh podman-machine-default \
  "podman build --pull=newer -t localhost/arkaos:dev /var/home/Ram/arkaos/"
```

**2. Produce the disk image:**

Kill any running boot-test VMs first — podman machine dies under memory pressure with two QEMU instances + a build.

```bash
podman machine ssh podman-machine-default "podman run --rm --privileged \
  -v /var/lib/containers/storage:/var/lib/containers/storage \
  -v /var/home/Ram/arkaos/config.toml:/config.toml:ro \
  -v /var/home/Ram/arkaos/output:/output \
  quay.io/centos-bootc/bootc-image-builder:latest \
  --type qcow2 --rootfs xfs localhost/arkaos:dev"
```

Output: `output/qcow2/disk.qcow2`

**3. Initialize swtpm (once per VM lifecycle):**
```bash
mkdir -p /tmp/arkaos-tpm
swtpm_setup --tpm2 --tpmstate /tmp/arkaos-tpm \
  --createek --decryption --create-ek-cert
```

**4. Boot the VM:**
```bash
cp /usr/share/edk2/x64/OVMF_VARS.4m.fd ./OVMF_VARS.4m.fd   # first run only

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
```

**5. Serial console:**
```bash
telnet localhost 4445   # login: ram / arkaos
```

---

## Demo: Browser Isolation Proof

This is the headline claim of Phase 4: a compromised browser cannot read user files.
Run this sequence in the VM serial console.

```bash
# Write a sentinel file to the real home directory
echo "topsecret" > ~/secret.txt

# 1. Can the sandbox read the sentinel?
firefox --shell -c 'cat ~/secret.txt 2>&1'
# → cat: /var/home/ram/secret.txt: No such file or directory

# 2. What is /home inside the sandbox?
firefox --shell -c 'grep " /home " /proc/self/mountinfo'
# → ... tmpfs on /home ...    (real home is hidden)

# 3. Are WiFi credentials visible?
firefox --shell -c 'ls /etc/NetworkManager 2>&1'
# → ls: cannot access '/etc/NetworkManager': No such file or directory

# 4. Is the arkad daemon config visible?
firefox --shell -c 'ls /etc/arkad 2>&1'
# → ls: cannot access '/etc/arkad': No such file or directory

# 5. What is /etc inside the sandbox?
firefox --shell -c 'ls /etc'
# → alternatives  fonts  hosts  ld.so.cache  ld.so.conf.d
#   nsswitch.conf  pki  resolv.conf  ssl
#   (nothing else — no credentials, no machine-id, no shadow)

# Verify the default launch path IS the sandbox
ls -la /usr/bin/firefox
# → /usr/bin/firefox -> firefox-sandbox
```

`firefox --shell` reuses the exact same bwrap args as a real browser launch —
there is no gap between what the proof tests and what ships.

Persistent Downloads directory is the only home subdirectory re-exposed:
```bash
firefox --shell -c 'ls ~/Downloads'   # writable inside sandbox
```

---

## arkad: Privacy Daemon

Source: `arkad/` — Rust, static musl binary, no tokio.

Four enforcers, verified active every 60s:

| Enforcer | What it does | Enforced via |
|----------|--------------|--------------|
| `mac.rs` | Randomize MAC on every connection | NM conf.d |
| `dns.rs` | DNS-over-TLS, Quad9 (9.9.9.9) | systemd-resolved conf.d |
| `hostname.rs` | Set hostname to `arka` | `hostnamectl` |
| `ipv6.rs` | IPv6 privacy extensions | `sysctl use_tempaddr=2` |

Config: `/etc/arkad/arkad.toml` — secure defaults baked in, works with no file.

```bash
# In VM: verify arkad is active
systemctl is-active arkad            # → active
journalctl -u arkad --no-pager -n 20 # → enforcement log
```

---

## Limitations

### Phase 3: measured boot is incomplete

PCRs 0-10 are measured by the firmware and captured by TPM2. PCRs 11-15
(kernel + initrd + cmdline, the useful ones for OS-level sealing) are dormant.

**Why:** `ConditionSecurity=measured-uki` in the relevant systemd unit is not
satisfied because GRUB loads the kernel directly, not via a UKI. The UKI
artifact exists in the image layer (`/usr/lib/modules/<kver>/<kver>.efi`) but
`bootupd` on CentOS Stream 10 has only a `grub2-static` component — there is
no systemd-boot component to stage the UKI to the ESP.

This means TPM-sealed disk encryption and remote attestation against OS state
are not available in the current build. The signed kernel chain is intact; only
the PCR-based sealing path is blocked.

**What this affects:** The composefs read-only rootfs and the bwrap sandbox
both work independently of PCRs 11-15. The integrity story is "signed,
immutable, composefs" — not "sealed to OS state."

### Phase 4: sandbox scope

- Network is host namespace (browser needs internet). arkad's DoT and IPv6
  privacy extensions operate at the kernel/NM layer — below bwrap — and remain
  active for all browser traffic.
- Browser profile (`.mozilla/`) lives in the `/home` tmpfs and is discarded on
  exit. No persistent browser state. This is intentional.
- Wayland/X11 socket isolation not tested (headless build).
- Only Firefox is sandboxed. Other apps run unsandboxed (Phase 4 scope).

### Research branch: `research/fedora-systemd-boot`

The hypothesis: Fedora's bootupd ships a systemd-boot component (unlike CentOS
Stream 10), and using `[install] bootloader = "systemd"` in a Fedora bootc base
would stage the UKI to the ESP on install, enabling PCRs 11-15 and completing
the measured-boot chain. The branch ports the current arkad + bwrap setup to a
Fedora bootc base, changes only the bootloader path, and documents the
pass/fail result. See `RESEARCH.md` on that branch.

---

## Files

```
Containerfile          multi-stage build (rust:alpine → centos-bootc:stream10)
arkad/                 privacy daemon source (Rust)
  src/main.rs          60s enforce loop
  src/enforcers/       mac, dns, hostname, ipv6
  arkad.service        systemd unit
  arkad.toml           config with secure defaults
arkaos-firefox         bwrap wrapper (IS /usr/bin/firefox in deployed image)
config.toml            bootc-image-builder config (qcow2, XFS)
PHASE3-FINDINGS.md     measured-boot investigation and blocked path analysis
PHASE4-SANDBOX.md      sandbox model, isolation proof, scope boundaries
```
