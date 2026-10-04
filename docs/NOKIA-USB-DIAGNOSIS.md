# Nokia USB boot diagnosis — 2026-10-05

Evidence: read-only extraction in `~/arkaos-diag/out`, collected with
`usb-diag.sh`. The collector used a read-only loop device and XFS `ro,norecovery`,
then unmounted it. This review reads the exported files only. Raw journals and
core dumps are not committed because they can contain personal information.

## Scope

The two retained boots are dated **2026-09-25**, on the Nokia PureBook
NKi510UL85S, BIOS N.1.02 (2020-10-23). They ran kernel
`6.19.14-101.fc42.x86_64` and systemd `257.13-1.fc42`.
They are evidence about the old Fedora 42 image, not the current Fedora 44 VM.
The third requested boot does not exist. Photos remain unavailable.

## Confirmed failure: serial-login crash loop

Both boots contain five `agetty` SIGSEGVs, all for
`serial-getty@ttyS0.service`. In boot 0:

- `boot0.txt:2`: the kernel command line includes `console=ttyS0`.
- `boot0.txt:1211`: the ttyS0 device condition is skipped.
- `boot0.txt:1784`: systemd nevertheless starts the serial-login service.
- `boot0.txt:1867`: terminal-name lookup returns `-22`, immediately followed by
  a null-address segfault. The core trace starts in `__strncmp_avx2`.
- `boot0.txt:2104`: the fifth restart reaches the service start limit.

The same sequence appears in boot -1. `coredumps.txt` lists ten `agetty` cores;
ABRT deduplicates them, explaining repeated notifications naming the first PID.
KWin and Plasma still start (`boot0.txt:2364,2431`). Neither exported journal
records a kernel panic, OOM kill, GPU hang, or USB I/O error. The last journal
ends without a complete shutdown, so absence of a recorded panic does not
exclude an unrecorded freeze or power loss.

**Strongly supported mechanism:** the forced serial console activates a login
path whose terminal lookup fails, then `agetty` dereferences invalid state.
This closely matches [util-linux issue 3304](https://github.com/util-linux/util-linux/issues/3304)
and its [upstream fix 3425](https://github.com/util-linux/util-linux/pull/3425).
The precise installed util-linux build and a symbolized core were not exported,
so matching that exact patch remains an inference, not a binary-level proof.
[systemd documents](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd-getty-generator.xml)
how a kernel serial console activates a serial getty.

The current `plymouth.ignore-serial-consoles` option controls splash behavior;
it does not remove `console=ttyS0` or disable agetty. The protected Containerfile
boot sections were not changed. For the Fedora 44 candidate, verify the installed
util-linux fix and test both a VM with serial hardware and the Nokia without a
usable ttyS0. Do not declare this fixed from a successful splash animation.

## Confirmed separate fault: primary GPT table checksum

`gpt.txt` reports both GPT headers valid, the primary partition-entry array's CRC
invalid, and the backup array valid. Its primary array begins at sector 2016,
which the tool also flags as an unusual placement. This explains why the
attached stick was visible without partition devices during inspection.

The evidence does **not** establish what damaged the table. The old boot journal
saw `sda1`, `sda2`, `sda3` and a clean XFS mount; it cannot date the later damage.
The report does not identify a misplaced backup header. Consequently, the
fresh-flash `sgdisk -e` procedure is not a sufficient diagnosis of this stick.
Preserve needed data and investigate the backup table before any repair or
reflash. No repair, reflash, partition write, or internal-disk operation was run.
See the [GPT fdisk recovery documentation](https://www.rodsbooks.com/gdisk/repairing.html).

## Other observations and limits

- The kernel reports an MMIO stale-data risk with SMT enabled, despite loading
  newer microcode and enabling CPU-buffer clearing. This is a residual security
  concern, not evidence of the getty crash cause. Review the
  [kernel mitigation guidance](https://www.kernel.org/doc/html/latest/admin-guide/hw-vuln/processor_mmio_stale_data.html)
  on dedicated ArkaOS hardware; no host firmware or SMT settings were changed.
- ACPI firmware errors, NFS initramfs warnings, and a read-only `/usr/local/sbin`
  tmpfiles warning appear; no causal link to the getty fault is established.
- Boot 0 records a session logout around 212 seconds, followed by SDDM returning
  and another login. Audio-device removal warnings coincide with the session
  transition; they do not establish Firefox sandbox audio failure.
- arkad starts and reports score 100 in this old image. That aggregate does not
  validate current controls. There are no Firefox/Capsule failures in these
  exported logs, so their hardware regression tests remain open.
- ABRT and DrKonqi crash-report components start, including a pending-report
  watcher. These lines do not prove anything was uploaded. Release validation
  must check crash-report consent and outbound behavior rather than claim that
  no telemetry is possible merely because arkad has no upload client.

## Evidence fingerprints (SHA-256)

```text
7e7fd25bc69b8486758ffa341975ae0c4974112246d4cfa940f3d95232078700  boot0.txt
53a26a3d80f022ee4b302d0942bf7f3661e5e2584d2982a88a8ca90ef3d4a733  boot-1.txt
e37583bb36b7700d6d2f32904e732bec4f24a9ae2a968a57b21b333e010a442a  gpt.txt
946811a596d6e3f3ebcc617000e39cc3d266ac102fffadb917fc1b5018b873fd  mount.txt
a01efc9e069a68343f0c035dc9aa783eba520f8f73dab6d5e834c05b65bc4b4f  coredumps.txt
```
