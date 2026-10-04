# ArkaOS hardening plan

Research and source review: 2026-10-04. The immediate target is a reliable,
inspectable developer preview with truthful protection status. Completion is
proved by tests of the deployed image as well as source tests.

## Lessons from other operating systems

These are engineering recommendations for ArkaOS, derived from official
documentation. They are not claims that ArkaOS already implements those systems'
protections.

| Reference | Mechanism documented by the project | Application to ArkaOS |
| --- | --- | --- |
| [Fedora Silverblue](https://fedoraproject.org/atomic-desktops/silverblue/) | Atomic OS updates take effect on reboot; previous versions can be restored; development tools can live in Toolbx. | Retain the bootc/Plasma foundation. Verify upgrade, rollback and user-data preservation using disposable disk files. Keep build dependencies outside the running desktop image. |
| [GrapheneOS](https://grapheneos.org/features) | Reduces attack surface, strengthens app isolation and uses verified boot to limit persistence. | Remove setup-only root privileges after onboarding. Validate each advertised control against its mechanism. Treat a read-only filesystem and cryptographic verification as separate properties. |
| [Qubes OS](https://doc.qubes-os.org/en/latest/developer/system/security-design-goals.html) | Separates activities into security domains. | Document the actual browser/home/socket boundary and test denied access. ArkaOS's browser sandbox does not provide Qubes-style VM isolation. |
| [NixOS](https://nixos.org/manual/nixos/stable/) | Declarative system configuration describes packages, services and users, with system generations available for rollback. | Make image inputs and versions traceable, use committed dependency locks, and minimize hidden first-login configuration. Preserve a tested recovery generation. |
| [ChromiumOS](https://www.chromium.org/chromium-os/chromiumos-design-docs/firmware-boot-and-recovery/) | Boot and recovery are designed together, including alternative firmware paths and verification decisions. | Test the failed-update and failed-boot paths before releasing an image. Plan signing and verification end to end, without changing the development host's firmware or bootloader. |
| [Ubuntu Core](https://documentation.ubuntu.com/core/explanation/recovery-modes/) | Distinguishes ordinary operation, recovery, installation and factory reset, with different data effects. | Give recovery operations clear consequences. Confirm user-data preservation independently of OS rollback. Keep destructive installation out of ordinary desktop actions. |
| [Flatpak](https://docs.flatpak.org/en/latest/sandbox-permissions.html) | Uses constrained apps and resource-specific portals; broad permissions expose more resources. | Parse actual permission sets. DRI is graphics access; PulseAudio includes microphone input and playback. Label broad device access accurately and prefer portals for individual resources. |

## First source hardening pass

- The privileged first-run helper rejects existing accounts, multiline usernames,
  invalid autologin flags, missing/short passwords, completed setup and concurrent
  setup. It validates before creating an account and removes the setup sudo rule
  after successful completion.
- The TUI delegates account creation to the same helper and stops on failure.
  It no longer has an independent password-reset path or reports every command
  as successful.
- Shared typed status predicates are used by the tray and agent. Only
  `Encrypted` confirms encrypted DNS. Browser isolation requires a recognized
  browser mode and `Active` sandbox state. Unexpected values remain unconfirmed.
- Agent test fixtures use the exact values produced by the daemon.
- The weekly report counts logged events. It no longer infers DNS uptime or
  browser launch counts from evidence the log does not contain.
- The source-check workflow runs isolated setup tests, reliability tests, agent
  tests, the shared-state/tray tests and a dashboard compile check using committed
  Rust lockfiles. Build artifacts are excluded from version control and local
  credential files are excluded from the image build context.

These are source changes. They do not change an already installed ArkaOS image.
If account creation succeeds but a later setup step fails, the partial account
is preserved and cannot have its password reset by retrying setup. Recovery of
that partial setup requires an explicit administrator operation. A resumable,
transactional first-run flow remains future work.

## Second source hardening pass

- arkad checks every control on each bounded retry interval. It serializes manual
  and periodic enforcement, propagates failed operations, bounds subprocesses
  and verification, and only records recovery after verification succeeds.
- DNS requires the resolved stub path and strict TLS on every reported DNS
  scope, including split-DNS links. This checks system-resolver policy; it does
  not prove reachability, query success, application-specific DNS or DNS uptime.
  Conflicting VPN settings stay unconfirmed without overwriting the VPN.
- MAC status compares active physical-interface addresses with their permanent
  addresses through NetworkManager. A changed locally administered address and
  the configured default are required. Existing connections are not disconnected
  automatically. This does not make a device untrackable.
- IPv6 checks per-interface kernel values, including defaults; failed commands
  no longer produce success logs. Browser checks require the expected wrapper
  target; executing and testing the browser boundary remains an image gate.
- Dashboard and tray polling bypass D-Bus caches, use deadlines and reconnect.
  Missing data replaces old successful indicators. Reliability snapshots have
  timestamps and become unavailable on sampling failure or after 30 seconds.
- Flatpak controls use real capability names and valid allow/deny options, parse
  complete entries from effective permissions, and reread after changes. They
  explicitly show direct grants: portals, broader filesystem paths, conditional
  permissions and custom installations are not a complete access audit.
  Audio-server access includes recording; DRI means graphics, not camera access.
- WiFi scanning and connection, permission operations and wallpaper changes run
  outside the GTK thread with deadlines. WiFi credentials use stdin rather than
  argv, and escaped SSIDs retain their actual names.
- Settings use Plasma's SDDM/screen-lock modules and wallpaper helper. The
  console utility updates ArkaOS's SDDM drop-in, validates the regular account
  and reports failures. Other SDDM configuration can override that preference.
- Model traffic is restricted to loopback HTTP, with localhost pinned, proxies
  disabled and redirects disabled. Answers use canonical tool facts, including
  actual write results. Piped input cannot approve writes. Audit records redact
  inputs, use private file permissions and serialize writers; approved intent
  is synced before a write runs. The local hash chain is not tamper-proof.
- Image builds use Rust lockfiles, preserve distribution password policy and
  generate the UKI after the final initramfs. Fedora 44 is the candidate build
  baseline. See [the migration and hardware gates](RELEASE-VALIDATION.md).
- New-user defaults turn off optional heavy window effects and file indexing.
  The image includes Fedora's zram defaults. Core desktop and privacy services
  do not require an AI model, GPU inference or a network connection to a model.

## Remaining release gates

### Security review follow-up, 2026-10-04

- Manual privileged enforcement now authorizes the real D-Bus sender through
  polkit. Read-only properties and automatic maintenance retain their behavior.
  Denial, missing authorization service and timeout do not invoke an enforcer.
- Firefox clears the inherited environment, drops capabilities, rejects
  Downloads symlinks and limits the Wayland socket name. Direct audio access
  still includes recording capability; it is not a microphone privacy barrier.
- KWin scripts use private temporary directories and validate window UUIDs
  before interpolation, including values obtained through journal parsing.
- The daemon and desktop lockfiles move `event-listener` to 5.4.2 for
  [RUSTSEC-2026-0221](https://rustsec.org/advisories/RUSTSEC-2026-0221.html).
- 8 daemon tests, 5 shared-state/window tests and 6 browser-wrapper tests pass.
  The private-bus test uses fake authorization and enforcement services. A real
  bubblewrap shell smoke test hid a synthetic environment secret and a private
  home sentinel while exposing its temporary Downloads directory.
- All 13 desktop workspace members and the assistant pass locked offline
  compile checks. Two existing dashboard GTK deprecation warnings remain.
- The concurrent setup draft also received an unprivileged preference-write fix
  and updated isolated fixtures; its 16 tests pass in the shared working tree.
  This setup integration is separate from the independent security commit.

These checks did not boot or alter the active VM. Test administrator prompts,
denied/cancelled repair, Firefox media/downloads and compositor actions in the
next guest image. A complete dependency advisory scan was not run. The updated
[threat model](THREAT-MODEL.md) records the boundaries and primary references.

### Image and hardware gates

1. Build the Fedora 44 candidate in an isolated builder and verify all package,
   service and kernel/UKI assumptions; pin reviewed image digests for a release.
2. Boot a disposable image and exercise first-run success/failure, networking,
   DNS packet capture, VPN/captive portals, MAC reconnects, disabled/missing
   interfaces, browser file/socket isolation, and service restarts.
3. Confirm installed `/usr` is read-only and verify atomic updates, failed
   updates, rollback and user-data preservation. The build recipe alone does
   not certify immutability or cryptographic boot verification.
4. Test fresh and upgraded Flatpak permissions, rejected overrides, portal
   prompts and audio/video hardware. Test Plasma KCM names, authorization,
   wallpaper handling, accessibility and every shipped app on the candidate.
5. Measure startup, idle RAM, memory pressure, thermals and graphics behavior
   across the hardware matrix. Bluetooth, sound and package-management actions
   still need a full responsiveness/deadline review.
6. Validate the changed local-model path against a running model and live
   daemon. Ensure audit records remain private and verify recovery after a
   crash between approved intent and completion.
7. Design and test disk encryption, signed updates and verified boot on
   disposable images or dedicated test hardware. Never operate on the host
   Windows partitions, EFI system partition, bootloader or firmware.

## Local source checks

```sh
python3 scripts/test-oobe-apply.py
bash -n arkaos-oobe-apply
bash -n arkaos-firstboot
cargo test --locked --manifest-path arkad/Cargo.toml
cargo test --locked --manifest-path arka-pulse/Cargo.toml --features dbus
cargo test --locked --manifest-path arka-agent/Cargo.toml
cargo test --locked --manifest-path arka-shell/Cargo.toml -p arka-shell-common -p arka-privacy-tray
```

The setup harness substitutes temporary paths and fake account/service
commands. It does not create host accounts or change host settings. Live model
tests are excluded by default and must be reported separately.

VM cleanup removed the previous Linux build machine and local firmware images.
Image-level verification needs a freshly created, isolated build/test environment
and disposable image files. It must never use a physical host disk as a target.

## 2026-10-05 work-item verification

- arkad exposes read-only `LastCheck`, Unix seconds of the last completed
  reconcile. Completed failures also advance it; cancellation does not. The
  tray polls every 30 seconds, requires age <90 seconds, and uses a neutral
  unknown icon when the timestamp is missing, zero, future or expired.
- `arka-update::status::parse_status` parses bootc v1 JSON without commands or
  UI changes. It preserves unknown version metadata, distinguishes staged
  downloads from restart-ready updates, handles queued rollback, and rejects
  malformed/non-bootc input. Rollback availability is not boot-success proof.
- Firefox explicitly selects the mounted PulseAudio compatibility socket and
  exposes that socket rather than its entire runtime directory. A read-only
  `pactl info` connection succeeded through the real wrapper with a temporary
  home on the development Linux host. This does not verify audible playback
  in the Nokia image.
- Capsule configures the user Flathub remote before installation, preserves
  command failures, displays launch failures, waits asynchronously, and queries
  Flatpak before marking an app installed. Regression tests use fake commands;
  a real install on the rebuilt image is still a release check.
- Local validation: all 13 shell crates built under `cargo test --workspace`
  (40 tests passed), 9 daemon tests passed using an isolated test bus, 7 browser
  wrapper tests passed, and 16 setup-helper tests passed. ShellCheck 0.11.0
  passed on all four requested shell helpers. Existing dashboard GTK
  deprecation warnings remain in the UI owned by the parallel work.
- CI now tests every shell crate, including OOBE, and checks all four shell
  helpers. Local results do not claim a hosted CI run or validate an image.
- README, website and documentation now explain that “Configured” means a
  setting is in place. DP1 screenshots are flagged as outdated, especially
  privacy-score and inferred DNS-coverage panels.

The original Nokia USB-boot crash logs/photos were not located. The historical
field notes report missing browser audio sockets, an absent Flathub remote and
backup-GPT placement after flashing a smaller raw image to a larger stick.
Those observations do not establish the cause of a kernel or desktop crash.
The USB documentation adds `sgdisk -e` followed by verification, for an explicitly
selected external test stick only. No physical disks were attached to test guests or modified
for these checks.

Upstream references: [bootc status schema](https://github.com/bootc-dev/bootc/blob/main/crates/lib/src/spec.rs),
[bootc rollback behavior](https://bootc.dev/bootc/man/bootc-rollback.8.html),
[PulseAudio client server selection](https://wiki.freedesktop.org/www/Software/PulseAudio/FAQ/),
[sgdisk options](https://www.rodsbooks.com/gdisk/sgdisk.html).

### Nokia evidence obtained later on 2026-10-05

The missing-journal limitation above is now resolved by a read-only USB export;
photos remain unavailable. [The diagnosis](NOKIA-USB-DIAGNOSIS.md) identifies ten
agetty serial-login crashes across two old Fedora 42 boots and a separate primary
GPT checksum failure. These findings do not validate the current Fedora 44 image.
The boot configuration ownership restriction and read-only USB scope remain in
force; neither the stick nor protected boot files were changed.
