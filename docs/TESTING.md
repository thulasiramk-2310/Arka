# Testing ArkaOS

The iron rule: **compile success ≠ runtime correctness.** Nothing is "done"
until it has been observed working in a booted VM, and nothing is committed
before that. Security claims additionally require a proof that exercises the
exact deployed artifact (same path, same args, same user).

## The standard verification pass

After every image build:

1. Boot the qcow2 (see BUILDING.md §3).
2. Watch the boot: Plymouth splash (triangles + ARKA + progress line) should
   appear within ~2 s and hand off to SDDM without a console flash.
3. Login screen: arkaos SDDM theme (dark, wordmark, clock). Log in.
4. Desktop: top panel + floating dock + wallpaper; open the launcher.
5. `telnet localhost 4445` (serial) and check the core services:

```sh
systemctl is-active arkad sddm power-profiles-daemon
bootc status            # composefs active, image digest
systemd-analyze         # boot time
cat /var/log/arkaos/privacy.jsonl | tail   # arkad enforcing
```

6. Privacy proofs (each phase's doc has the full procedure):
   - sandbox: sentinel in real home unreadable via `firefox --shell`
   - DNS: `resolvectl status` shows DoT to 9.9.9.9
   - MAC randomization conf present; hostname is `arka`

## Driving the VM without a keyboard (headless/CI)

QEMU monitor on :4444 accepts `sendkey` and `screendump`:

```sh
printf 'screendump /tmp/shot.ppm\n' | nc -q1 localhost 4444
magick /tmp/shot.ppm shot.png
```

sendkey gotchas: `_` = `shift-minus`, `[` = `bracket_left`, capitals =
`shift-<letter>`. Serial login (ram/arkaos) is more reliable than sendkey for
anything longer than a password.

GUI apps that shell out to hardware tools (bluetoothctl, brightnessctl) hang
forever in a VM with no such hardware — anything of the sort must run under
`timeout` (see arka-bar's `cmd_stdout()` for the pattern).

## Quality engineering (the current phase)

Feature set is frozen. The work is now reliability, performance, consistency.

### Boot reliability — `scripts/qa-boot-loop.sh`

Boots the image N times headless, records: reached graphical.target (y/n),
time to graphical.target, failed units, arkad active. Any failure is a bug.
Target: 100/100 clean boots before Developer Preview 1.

### Memory budget

On a settled desktop (2 min after login), per-service RSS via serial:

```sh
for u in arkad sddm plasmashell kwin_wayland; do
  systemctl show -p MainPID --value $u 2>/dev/null | xargs -I{} \
    awk '/VmRSS/{print "'$u'", $2/1024 " MB"}' /proc/{}/status 2>/dev/null
done
```

Budgets (fail the pass if exceeded): arkad ≤ 30 MB, each arka GTK app ≤ 60 MB,
whole desktop idle ≤ 2.5 GB.

### Performance

- `systemd-analyze` + `systemd-analyze blame` every pass; boot target ≤ 20 s
  in the VM, no unit > 5 s.
- App cold-start: launcher, dashboard, capsule, settings each ≤ 1 s to first
  frame (eyeball via screendump timestamps until we instrument).

### Consistency audit

Open every shipped surface side by side (screendumps): SDDM → desktop →
launcher → dashboard → capsule → settings → perms → wifi → sound → update →
Dolphin → System Settings. For each: spacing on the 8px grid, radii from the
design-system table, colors are token values (pick pixels and compare), Inter
everywhere, one icon language per row. Any deviation is a bug against
`arka-design-system/README.md`.

### User testing

Hand the VM to a non-technical person with one sentence: "use this computer."
Every place they stall is a bug. No explaining, no steering.
