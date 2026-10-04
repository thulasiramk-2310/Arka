# Release and hardware validation

ArkaOS aims for an immutable system image, privacy controls users can inspect,
and a usable desktop across modest and powerful PCs. These are acceptance
criteria, not a claim that the current source has passed them all.

## Validation recorded 2026-10-04

- 119 automated source tests passed: 7 privacy-daemon, 35 reliability,
  50 local-assistant, 12 shared-state/tray, 3 permission/WiFi parser and 12
  isolated setup tests. Two live-model tests were skipped.
- All 13 desktop workspace members passed `cargo check --locked`; the
  dashboard still emits two pre-existing GTK deprecation warnings.
- `arkad` and `arka-pulsed` passed static musl release builds in rootless
  containers with committed lockfiles and a 3 GB memory limit.
- The complete image build was stopped during Fedora desktop-builder dependency
  installation to preserve disk space while another VM build was active. No
  final image, VM boot, upgrade/rollback, hardware or live-model result is claimed.
- Temporary containers and images from that stopped build were removed by
  explicit ID. The active VM, Windows filesystems and host boot settings were
  left untouched. No host privacy enforcer or installer was executed.

### Desktop follow-up during independent image testing

The sound and update changes below were made after the independent image build
and boot test started. Those test results must be tied to their source revision;
they do not automatically validate these later changes.

- Nine additional isolated regression tests pass: six audio-state/command
  tests and three update-command tests. They use fake command responses and a
  shell function replacing sudo; they do not change host audio or deployments.
- Both affected apps pass `cargo check --offline --locked` using a separate
  build directory and one build job. No image or VM build was started here.
- Audio queries and writes run in a serialized background worker with bounded
  subprocesses. The panel rereads state after writes, reports failures and
  represents missing devices as unknown. Slider requests are coalesced and
  external state is refreshed while the panel is open. Controls cover only
  the default devices, not every microphone or application recording stream.
- The updater reads the image-owned version file, retains failed command
  output, reports terminal-launch failures and removes unsupported signing
  and data-preservation claims. Rollback explicitly describes its staged-update
  and user-file consequences.

On the next candidate image, test absent/stopped PipeWire, device hotplug,
external mute changes, rapid slider input and microphone readback. Test update
authentication failure, an unreachable registry and an unavailable rollback
deployment. The terminal must preserve the error in each case. None of these
guest integration tests is claimed by the source tests.

Behavior references: [WirePlumber wpctl](https://pipewire.pages.freedesktop.org/wireplumber/man/wpctl.html),
[bootc rollback](https://bootc.dev/bootc/man/bootc-rollback.8.html).

## Candidate baseline and migration

The Containerfile now defaults both the GTK builder and bootc runtime to Fedora
44. Fedora 42 is beyond its update/support window. References:
[Fedora 44 release announcement](https://fedoramagazine.org/announcing-fedora-linux-44/),
[Fedora 42 end-of-life announcement](https://www.mail-archive.com/announce%40lists.fedoraproject.org/msg01239.html).

This is a source-level candidate. No installed system has been rebased. The
Fedora 44 image, first boot and rollback have not been validated in this pass.
The image owns `/usr`; account data, `/var`, and administrator configuration
remain persistent. No data migration or partition operation is part of this
change. Build the whole image so that services and desktop consumers move
together. Older consumers are not certified with the new service behavior;
new consumers show missing timestamp properties as unavailable.

Forward path: build and tag a separate candidate image, retain the previous
image and disk snapshot, then boot a disposable copy and run the gates below.
Only after passing should an installed ArkaOS guest test a bootc upgrade.
Rollback path: use the retained guest deployment/snapshot and verify account
data independently. OS rollback does not itself roll back user data.

For investigation, `--build-arg FEDORA_VERSION=42` selects the previous build
baseline. It is not a supported release target. Release builds must record and
pin reviewed container digests and package/toolchain provenance; `--locked`
alone does not make the entire image reproducible.

## Hardware targets

The current build is x86-64 and uses x64 UEFI artifacts. ARM, 32-bit machines,
legacy BIOS and every GPU/network adapter are not established support targets.
No TPM or local language model is required for the normal desktop. Do not
reduce privacy controls automatically on smaller machines.

| Test target | RAM / virtual CPUs | Acceptance |
| --- | --- | --- |
| Constrained, exploratory | 2 GB / 2 | First-run completion, login, one browser workload, responsive settings under memory pressure; may require a separate lighter desktop if it fails |
| Baseline candidate | 4 GB / 2 | Full desktop workflow, WiFi/permissions operations, updates and rollback |
| Mainstream | 8 GB / 4 | Multitasking, suspend/resume, peripherals, stable idle behavior |
| Higher capacity | 16–32 GB / 8 | Multiple applications, graphics acceleration, optional local assistance tested separately |

These are proposed test configurations, not measured minimum requirements.
Fedora generally recommends 4 GB RAM and 40 GB storage:
[Fedora KDE downloads](https://fedoraproject.org/kde/download/).
ArkaOS must measure additional space for retained deployments and updates.
Record cold boot time, time to usable desktop, idle RSS, swap usage, memory
pressure, UI stalls and workload completion. Repeat with software graphics and
representative Intel, AMD and NVIDIA hardware before claiming support.

The recipe includes [Fedora zram defaults](https://fedoraproject.org/wiki/Changes/SwapOnZRAM)
and disables optional wobbly/magic-lamp effects and file indexing for new users.
Those defaults reduce avoidable work; they are not performance measurements.
Users can enable effects and indexing in Plasma settings.

## Image acceptance gates

Use only disposable qcow2 images in an isolated guest. Never pass physical host
disks, Windows partitions, host EFI variables or a host EFI system partition to
build, install or recovery commands. No host bootloader operation is required.

1. Build the image and pass `bootc container lint`; confirm the final UKI embeds
   the final initramfs, the expected kernel and the final OS identity.
2. Complete first-run setup; test rejected existing accounts, invalid passwords,
   concurrent requests and partial failure without resetting existing users.
3. Confirm the deployed `/usr` is read-only, ordinary users cannot alter system
   binaries, and `/home`, `/var` and intended `/etc` changes survive updates.
4. Exercise upgrade, interrupted update, failed boot and rollback. Compare
   checksums of test user data before and after. Do not infer preservation from
   a successful boot alone.
5. Capture DNS traffic in the guest and test plaintext/split-DNS conflicts,
   captive portals, blocked TLS, offline operation and VPNs. Confirm the UI
   marks unverified controls clearly. Reconnect network devices to verify MAC
   changes; ensure periodic checks do not disconnect working sessions.
6. Stop/restart services and interrupt sampling. Old green indicators must
   disappear. Confirm "Fix All" reports failed controls and retries occur even
   when the aggregate score remains unchanged.
7. Test the browser's actual file, device and socket boundaries, Flatpak grants,
   portal prompts, denied operations, WiFi secrets and Plasma integrations.
8. Run the hardware matrix and record results and limitations before publishing.

Signing, Secure Boot, disk encryption and end-to-end image verification require
separate design and recovery tests. Read-only system files alone do not prove
those properties or prevent persistence in writable user/configuration data.

## Nokia regressions required on the Fedora 44 candidate

The [saved Nokia journals](NOKIA-USB-DIAGNOSIS.md) prove repeated serial-getty
crashes in a Fedora 42 image. A working splash or active graphical target is
insufficient. In the disposable ArkaOS guest, capture these read-only checks:

```sh
rpm -q util-linux-core systemd kernel-core
cat /proc/cmdline
systemctl --failed --no-pager
journalctl -b -u serial-getty@ttyS0.service --no-pager
journalctl -b -t systemd-coredump --no-pager
```

Record package versions, image digest and boot ID. Require no new getty cores,
terminal-name failures or restart-limit failures. Repeat with serial hardware
available in QEMU and on the Nokia without a usable ttyS0. Retain serial access
for VM diagnostics; do not globally mask login services to hide errors.
Protected boot configuration remains under its existing owner until integration.

For fresh disposable USB media, verify the flashed GPT before booting. A primary
CRC failure with a valid backup is distinct from simple backup-header relocation.
Actual data-preserving repair of an existing stick is a separate operation.

Check Firefox audible playback and Capsule install/retry on the image; neither
is established by the old Nokia journals. Review ABRT/DrKonqi consent settings
and observe outbound reporting in the isolated guest before making telemetry
claims. Evaluate residual CPU-mitigation warnings per hardware target rather
than silently changing host firmware or disabling SMT on every machine.
