# Backend status

Written 2026-11-15 from reading the source, the Containerfile and the docs in this
repository at `f98f395`, and from running the test suites on the development laptop.
The inventory rows come from reading, not from a new run: where a row says "verified",
the evidence is the dated entry in [RELEASE-VALIDATION.md](RELEASE-VALIDATION.md) or
[HARDENING.md](HARDENING.md). Only the three changes listed under "Done" were run on a
booted image, in a virtual machine.

Status words: **Complete** (built and has recorded evidence), **Partial**, **Missing**,
**Base** (left to Fedora, systemd or Plasma on purpose; ArkaOS adds nothing of its own),
**Not checked** (not looked at closely enough to say).

## Inventory

| # | Area | What exists | Status | Gap | Security note | Priority |
|---|------|-------------|--------|-----|---------------|----------|
| 1 | Boot and service lifecycle | systemd; greenboot health checks with two ArkaOS drop-ins; `arka-arm-boot-counter`; Plymouth theme; arkad is `Type=notify` with a 120 s watchdog | Partial | `arka-pulsed` has no watchdog; UKI and measured boot (PCR 11 to 15) are blocked on the platform (PHASE3-FINDINGS.md) | Boot chain is not verified end to end | Medium |
| 2 | Sessions and sign-in | SDDM theme; first-boot wizard in its own login session with a narrow sudo rule (`arkaos-oobe-apply`); polkit action for arkad | Complete | Only one laptop and virtual machines tested | sudo rules are the privileged surface; `test-oobe-apply.py` covers the helper | Low |
| 3 | Processes and resources | `zram-generator-defaults` | Base | No ArkaOS policy for out-of-memory handling or per-app limits; whether `systemd-oomd` is active on the image is not checked | None new | Low |
| 4 | D-Bus services | `org.arka.arkad` (read-only properties, one `EnforceAll` method behind polkit); `org.arka.pulse` (properties only); typed client enums in `arka-shell-common` | Complete | The interfaces are documented only in code; no interface version property | Bus policy files allow exactly the members above | Medium |
| 5 | Hardware and devices | Fedora kernel, udev, bluez, brightnessctl; kernel hardening files in `kernel/` | Base | No firmware-update path chosen; hardware matrix is one Nokia laptop and one ROG laptop | Kernel arguments not re-verified on the current image | Medium |
| 6 | Files, storage, mounts | composefs root; installer ISO with disk encryption by default | Partial | No fs-verity (kernel lacks it), so file contents are not cryptographically pinned; installer hazards with dual boot are untested | Encryption verified in a virtual machine only | Medium |
| 7 | Network and DNS | NetworkManager; arkad enforcers for MAC randomisation, DNS over TLS, IPv6 privacy addresses; `arka-dns-after-clock` | Complete | Strict versus opportunistic DNS over TLS is an open decision; captive portals and VPNs need a test pass | arkad never rewrites a user's VPN DNS | Medium |
| 8 | Sound, display, input | PipeWire, WirePlumber, KWin | Base | Firefox sandbox still has direct audio access including recording | Stated in HARDENING.md | Low |
| 9 | App discovery, install, launch | Flatpak; `apps/arka-install` with tests; Capsule store front end | Partial | Capsule was reported broken on real hardware (2026-09-13) and has not been re-tested since | Flatpak permissions on fresh and upgraded installs untested | High |
| 10 | Permissions and sandboxing | Firefox in bubblewrap (verified); `arka-perms` for Flatpak permissions; privacy dots from PipeWire | Partial | Only Firefox is sandboxed by ArkaOS itself; the two daemons are confined from today, other ArkaOS units are not reviewed | arkad still runs as root with network access | Medium |
| 11 | Privacy enforcement | arkad: six controls, re-checked every 10 to 60 s, drift and recovery events | Complete | Settings file values were passed on unchecked (fixed today); no way to change settings over D-Bus, by design | Root daemon; inputs now validated | Done today |
| 12 | Immutable system | bootc image, read-only `/usr` | Partial | Same as row 6 | "Immutable" means read-only at run time, not tamper-proof | Medium |
| 13 | Updates and signatures | Image signed in CI with cosign; `policy.json` on the image requires the signature; `bootc-fetch-apply-updates` without automatic reboot; `arka-bootc-status` publishes status as a file | Complete | Key rotation and a lost-key plan are not written down | The policy file is the only enforcement point | Medium |
| 14 | Deployments and rollback | greenboot with boot counter; automatic rollback verified in a virtual machine and on one laptop (2026-10-11, 2026-10-12) | Complete | One hardware model | A failed finalise no longer downgrades a healthy system | Low |
| 15 | Settings that persist | `/etc/arkad/arkad.toml`; Plasma and app settings in the home folder; `/etc` merges across updates | Partial | No schema or migration for `arkad.toml` when keys change | Now validated on load | Low |
| 16 | Notifications and events | Plasma notifications; `arka-privacy-tray` reads arkad's event file | Complete | Event file grew without limit (fixed today) | World-readable by design; contains setting names only | Done today |
| 17 | Logs, diagnostics, crash recovery | journald; `/var/log/arkaos/privacy.jsonl`; services restart on failure | Partial | No single command that gathers a support report; the crash-dump viewer's launcher entry is hidden, and whether crash dumps are kept was not checked | A report tool must not collect personal data | High |
| 18 | Monitoring and health | `arka-pulse`: read-only sampler and daemon, 31 tests | Complete | Predictions stay labelled as estimates; analyser for field data not built | Read-only; sandboxed from today | Done today |

## What depends on what

```
kernel + systemd + composefs/bootc
        │
        ├── NetworkManager, systemd-resolved, firewalld ──► arkad ──► dashboard, tray, status pill
        ├── /proc, /sys ──────────────────────────────────► arka-pulsed ──► health panel
        ├── bootc + policy.json + greenboot ──────────────► arka-bootc-status ──► Updates app
        └── logind + SDDM ──► first-boot wizard ──► Plasma session ──► Flatpak apps, Firefox sandbox
```

arkad needs the three network services before it; nothing else in ArkaOS needs arkad to
start. The desktop apps degrade to "unavailable" when a daemon is missing.

## Order of work

1. **Daemon inputs and logs** (done today, below).
2. **Service sandboxing**: both daemons done today.
3. **Support report**: one command that writes service states, the last boot's errors,
   bootc status and arkad's state to a file the user can read before sharing.
4. **App path**: re-test Capsule and Flatpak permissions on a current image; fix or remove.
5. **Documented D-Bus contract** with an interface version, so shells can check it.
6. **Settings schema** for `arkad.toml` with a version key.
7. Platform items that wait on Fedora: fs-verity, UKI, measured boot.

## Done on 2026-11-15 (not committed)

- **arkad settings are checked on load** (`arkad/src/config.rs`). The DNS server must be an
  IP address, the TLS name a host name, the computer name one lower-case label. Anything
  else is replaced by the default with a warning. Before this, a line break in the DNS
  values would have added settings to the resolver file, and a computer name starting
  with a hyphen would have been read by `hostnamectl` as an option. The file is writable
  only by root, so this closes a mistake path more than an attack path.
- **The event file is bounded** (`arkad/src/log.rs`). At 1 MiB it is renamed to
  `privacy.jsonl.1` and started again. Text is escaped so each line stays valid JSON.
  The dashboard and tray read only the current file, so right after a rotation their
  event list is short.
- **`arka-pulsed` is confined** (`arka-pulse/arka-pulsed.service`): read-only system,
  no home, no devices, no network, no capabilities. `systemd-analyze security` exposure
  went from 9.4 to 1.3.
- **`arkad` is confined** (`arkad/arkad.service`): read-only system except the three
  settings folders it manages and its log folder; no home, private `/tmp`, no kernel
  module or clock changes. Exposure went from 9.4 to 7.4. It stays at 7.4 because the
  daemon needs the network, `/proc/sys` and namespaces, and runs as root.

## Test results

| Suite | Result |
|-------|--------|
| `arkad` (`cargo test`) | 26 passed (21 before, 5 new) |
| `arka-pulse` (`cargo test`) | 31 passed |
| `scripts/test-*.py`, `apps/test_arka_install.py` | 12 suites pass; 20 cases skip for missing tools |

### In a virtual machine

Stick image built from `5e17879` (Fedora 44, SELinux enforcing), booted in QEMU with
`-snapshot`, 2026-11-15. The new unit files were placed in `/etc/systemd/system`; the
new arkad was a statically linked glibc build put in place with `bootc usr-overlay`
(the image itself builds arkad against musl).

| Check | Result |
|-------|--------|
| `arka-pulsed` with the new unit | Active, no restarts, `org.arka.pulse` properties readable and refreshed every 10 s |
| Settings file with line breaks in the DNS values and `--transient` as the name | Three warnings in the journal; resolver file and computer name unchanged from the defaults |
| Event file filled to 1 MiB, then arkad restarted | Moved to `privacy.jsonl.1`; new file started with valid lines |
| `arkad` with the new unit, all managed files deleted (including the resolver folder), IPv6 value and computer name changed, then started | All six controls confirmed, files recreated |
| Same changes made while `arkad` was running | Repaired at the next check; drift and recovery events logged |
| Name lookup after all of the above | Answered over the encrypted transport |
| Failed units | None |

## Not verified

- The image's own musl build of arkad with these changes; that needs an image build.
- Real hardware, a reboot with the new units, the `EnforceAll` method with its
  administrator prompt, and the Firefox sandbox itself under the confined arkad
  (only arkad's own check of it, which reported Active).
- A VPN or captive portal with the confined arkad.
