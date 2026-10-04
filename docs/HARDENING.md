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

## Release gates, in priority order

1. **Effective privacy controls.** Check child exit statuses and deadlines;
   verify effective resolver routing and active network connection settings;
   retry failed controls independently of aggregate score. Report recovery only
   after observing success. Test blocked DNS, conflicting connection settings,
   missing hardware and repeated enforcement failures in the guest.
2. **Fresh, consistent status.** Emit D-Bus property changes or bypass proxy
   caching for polling. Include timestamps and availability in snapshots.
   Test daemon disconnects, restarts and stale telemetry. Unknown status must
   never keep the last successful green display indefinitely.
3. **Least-privilege app controls.** Correct Flatpak permission labels, parsing,
   effective overrides and error handling. Use a protected channel for WiFi
   secrets. Move hardware calls off the GTK thread and give them deadlines.
   Finish the SDDM and Plasma migration for autologin and wallpaper settings.
4. **Grounded local assistance.** Enforce loopback-only model transport;
   bind generated claims to typed facts and their domains; record write intent
   before execution; redact audit inputs centrally and serialize concurrent
   audit writers. Test redirected endpoints, fabricated claims and write failures.
5. **Reproducible image and recovery.** Pin reviewed image/toolchain inputs,
   build with committed lockfiles, and retain artifact provenance. Run clean
   first-boot, update, rollback and user-data-preservation tests on disposable
   images. Source tests alone do not prove these paths work.
6. **Disk and boot protection.** Design disk encryption and image verification
   against documented hardware support and recovery requirements. Stage this
   work in a guest or dedicated test machine. Do not repartition, format, mount
   Windows filesystems, install a host bootloader or change host firmware.
7. **Desktop release quality.** Exercise every shipped app with and without
   hardware, check accessibility and keyboard flows, and measure idle memory,
   boot time and cold-start time. Keep the Plasma foundation while proving
   first-party app behavior.

## Local source checks

```sh
python3 scripts/test-oobe-apply.py
bash -n arkaos-oobe-apply
bash -n arkaos-firstboot
cargo test --locked --manifest-path arka-pulse/Cargo.toml
cargo test --locked --manifest-path arka-agent/Cargo.toml
cargo test --locked --manifest-path arka-shell/Cargo.toml -p arka-shell-common -p arka-privacy-tray
```

The setup harness substitutes temporary paths and fake account/service
commands. It does not create host accounts or change host settings. Live model
tests are excluded by default and must be reported separately.

VM cleanup removed the previous Linux build machine and local firmware images.
Image-level verification needs a freshly created, isolated build/test environment
and disposable image files. It must never use a physical host disk as a target.
