# Phase 4 — Default-on Browser Sandboxing

## Model

Firefox is sandboxed via bubblewrap (bwrap) by default. The wrapper IS
`/usr/bin/firefox` in the deployed image — there is no unsandboxed launch path.

**Why bwrap over Flatpak:** Flatpak + Firefox runtime adds ~1GB to an already 2GB
image and requires network access at build time or first boot. bwrap is ~100KB,
available in CentOS Stream 10 BaseOS, and the sandbox profile is a shell script
that is version-controlled, auditable, and baked into the image layer. Flatpak is
the right answer for a desktop with a package manager; bwrap fits an immutable
bootc image where every artifact is baked in.

## Interception mechanism

`/usr/bin/firefox` → symlink → `/usr/bin/firefox-sandbox` (the bwrap wrapper)

The real Firefox ELF is at `/usr/bin/firefox-unwrapped`. The rename was done at
image build time (`mv /usr/bin/firefox /usr/bin/firefox-unwrapped`). Because the
root filesystem is a composefs overlay (`ro` mount), these paths are immutable at
runtime — there is no way to replace the symlink or access the unwrapped binary
as the default launch target without breaking out of the immutable root.

Desktop `.desktop` launchers that call `/usr/bin/firefox` (absolute path) land on
the sandbox wrapper. PATH-based invocations also resolve to the wrapper.

## Sandbox profile (`/usr/bin/firefox-sandbox`)

```
Filesystem binds (all read-only unless noted):
  /usr /lib /lib64 /bin /sbin    — system binaries and libraries
  /etc (tmpfs + allowlist only)  — see below
  /dev (dev)                     — device nodes
  /proc (proc)                   — process info
  /tmp (tmpfs)                   — ephemeral scratch
  /run (tmpfs)                   — no D-Bus session access
  /home (tmpfs)                  — real home HIDDEN
  /root (tmpfs)                  — real root home HIDDEN
  ~/Downloads (bind rw)          — only home subdir re-exposed

/etc allowlist (everything else is hidden):
  resolv.conf, hosts, nsswitch.conf — name resolution
  ssl/, pki/                        — CA certificates for HTTPS
  fonts/                            — font rendering
  localtime                         — timezone
  ld.so.cache, ld.so.conf.d        — dynamic linker
  alternatives/                     — RHEL alternatives symlinks

Hidden from /etc (confirmed absent inside sandbox):
  NetworkManager/     — WiFi credentials not visible to browser
  arkad/              — privacy daemon config not visible
  machine-id          — unique host identifier not exposed
  shadow, passwd      — not needed by browser

Namespace isolation:
  --unshare-pid       — isolated PID namespace
  --unshare-ipc       — isolated IPC
  --unshare-uts       — isolated hostname
  --new-session       — new session ID (no terminal escape)
  --die-with-parent   — sandbox dies when wrapper exits

Network: host network namespace (browser needs internet). arkad's DNS-over-TLS
enforcer (Quad9) and IPv6 privacy extensions remain active — they operate at the
kernel/NetworkManager layer, below the bwrap sandbox boundary.
```

Browser profile data (`.mozilla/`) writes to the `/home` tmpfs and is discarded
on process exit. No persistent browser state accumulates on disk. This is a
deliberate privacy property, not a limitation.

## Test mode

```bash
firefox --shell          # interactive bash in the identical sandbox
firefox --shell -c 'cmd' # non-interactive command in the identical sandbox
```

The `--shell` mode uses the exact same `BWRAP_ARGS` array as the browser launch.
It exists for isolation testing and is not a privilege-escalation vector (the
sandbox is equally restrictive for bash as for Firefox).

## Isolation proof (verified in VM, 2026-06-08)

Setup:
```bash
echo "topsecret_1780909961" > ~/secret.txt   # written to real /var/home/ram/
```

Inside sandbox (`firefox --shell -c '...'`):
```
=== home ls ===
total 0
drwxr-xr-x. 2 1000 1000 40 Jun 8 09:12 .
drwxr-xr-x. 15 1000 1000 300 Jun 8 09:12 ..

=== secret read ===
cat: /var/home/ram/secret.txt: No such file or directory

=== home mountinfo ===
656 574 0:69 / /home rw,... - tmpfs tmpfs rw,...
658 574 252:4 /ostree/.../ram/Downloads /var/home/ram/Downloads rw,... - xfs ...

=== etc contents ===
alternatives fonts hosts ld.so.cache ld.so.conf.d nsswitch.conf pki resolv.conf ssl

=== NM in etc ===
ls: cannot access '/etc/NetworkManager': No such file or directory

=== arkad in etc ===
ls: cannot access '/etc/arkad': No such file or directory
```

After sandbox exit: `~/secret.txt` unchanged in real home. Sandbox writes went to
tmpfs and were discarded.

**The proof uses `firefox --shell`, not a hand-reconstructed bwrap invocation.**
The sandbox args are a single array in the wrapper; `--shell` reuses them directly.
There is no gap between what was tested and what ships.

## Regression checks (same boot, verified)

- `systemctl is-active arkad` → `active` (Phase 2 privacy enforcers intact)
- `mount | grep composefs` → `ro` overlay (Phase 3 composefs rootfs intact)

## What this demonstrates vs. what it is not

**Demonstrates:**
- A compromised browser process cannot read files from the user's home directory
- WiFi credentials (NM), daemon config (arkad), and host identity (machine-id) are
  not visible to the browser
- The sandbox is the only available launch path — no unsandboxed escape hatch

**Not yet implemented (known scope boundaries):**
- Persistent encrypted browser profile (Downloads are unencrypted)
- Wayland/X11 socket isolation (no display server in headless test)
- D-Bus session isolation (--tmpfs /run hides session bus; GUI use would need portals)
- Per-app sandboxing beyond Firefox (Phase 4 scopes to one reference app)
- TPM-sealed browser profile (requires PCRs 11-15, which are blocked by Phase 3
  platform limitation — see PHASE3-FINDINGS.md)
