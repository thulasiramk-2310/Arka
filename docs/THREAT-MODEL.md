# Threat Model (DP1)

What ArkaOS defends against today, what it doesn't, and what's planned.
Honesty over marketing: a privacy OS that overstates its protections is worse
than no privacy OS.

## In scope — defended today

**Network observers (ISP, local network, WiFi snooping)**
- DNS queries encrypted (DoT to Quad9) — no plaintext hostname leaks.
- MAC address randomized per connection — no cross-network device tracking.
- Generic hostname (`arka`) — no personal identifier broadcast.
- IPv6 privacy addresses — no EUI-64 hardware-derived addresses.
- Enforced continuously: drift is detected within 60 s and reverted (arkad).

**Malicious or compromised websites (browser as attack surface)**
- Firefox runs in bubblewrap: real home invisible (tmpfs), `/etc` allowlisted
  (no WiFi credentials, no machine-id, no arkad config), no D-Bus, no session
  persistence; only `~/Downloads` crosses the boundary.
- A browser exploit gains a throwaway environment, not your files.

**Persistence and system tampering**
- Root filesystem is read-only (composefs); the deployed image cannot be
  modified at runtime. Malware cannot patch system binaries or units.
- Updates are atomic; a bad or tampered update is rolled back as a unit.
- Boot chain measured into TPM PCRs 0–10.

**Data-hungry defaults**
- Zero telemetry in the OS. Every privacy-relevant event is logged locally
  (`privacy.jsonl`) for the user's own dashboard, never transmitted.

## Out of scope — NOT defended in DP1

- **Physical attackers / evil maid:** no disk encryption yet (TPM-sealed LUKS
  is blocked on the measured-boot gap, PCRs 11–15 — see PHASE3-FINDINGS.md).
  A stolen disk is readable. This is DP-series' most important known gap.
- **Kernel or firmware exploits:** we ship Fedora's kernel; no additional
  kernel hardening (lockdown tuning, hardened allocator) yet.
- **Non-browser applications:** only Firefox is sandboxed. Apps you install
  run with normal user permissions (Flatpak apps carry their own sandbox).
- **Traffic analysis / global adversaries:** no Tor/VPN integration; your IP
  is visible to sites you visit. DoT hides *what* you resolve from the path,
  not *that* you talk to Quad9.
- **Supply chain above us:** we trust Fedora's package signing and the
  container registries used at build time.

## Trust boundaries

```
user data (~)   ←  bubblewrap  →  browser
system image    ←  composefs (ro) + atomic updates  →  runtime
network         ←  arkad enforcement (DoT · MAC · hostname · IPv6)  →  world
```

## Planned hardening (post-DP1, in priority order)

1. TPM-sealed full-disk encryption (needs systemd-boot/UKI measured path).
2. Per-file integrity (fs-verity pinning in composefs).
3. Sandboxing for more first-party surfaces.
4. Optional network-level protections (VPN killswitch integration).
