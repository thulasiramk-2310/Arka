#!/bin/bash
# run.sh ISO OUT: the live ISO's end-to-end test (run as root, needs KVM and vsock).
#  1. boots the ISO with Secure Boot on and a software TPM 2.0 (swtpm), next to a fake
#     Windows disk (windows-disk.sh);
#  2. over SSH (vsock), runs Arka Install's backend as the screens do: probe, plan
#     "next to Windows" encrypted with the TPM and a PIN, apply with the PIN on stdin;
#  3. checks the Windows partitions, data and boot file are unchanged (check-windows.sh);
#  4. boots the installed disk with the same TPM, types the PIN, checks the system and
#     the disk's keys (TPM and recovery key only);
#  5. boots it again without a TPM and types the recovery key.
# Screenshots, logs and results go to OUT. Exits non-zero at the first failure.
# OVMF_CODE / OVMF_VARS: Fedora's Secure Boot firmware (OVMF_*_4M.secboot.qcow2).
set -euo pipefail
ISO=$(realpath "$1"); OUT=$(realpath -m "$2"); H=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d); mkdir -p "$OUT"
PIN="246810"
log() { echo "== $(date -u +%T) $*" | tee -a "$OUT/steps.txt"; }

ssh-keygen -q -t ed25519 -N '' -f "$W/key"
cred=$(printf 'd /root/.ssh 0700 root root -\nf+ /root/.ssh/authorized_keys 0600 root root - %s\n' "$(cat "$W/key.pub")" | base64 -w0)
cp "$OVMF_VARS" "$W/vars.qcow2"
bash "$H/windows-disk.sh" "$W/win.raw" > "$OUT/windows-disk.txt"
{ for p in 1 2 3 4; do echo "== $p"; sgdisk -i $p "$W/win.raw"; done; echo "== end"; } > "$W/gpt-before"
cp "$W/win.raw.sums" "$OUT/windows-sums.txt"

mkdir -p "$W/tpm"
swtpm_setup --tpm2 --tpmstate "$W/tpm" --createek --create-ek-cert --create-platform-cert --overwrite > "$OUT/swtpm-setup.txt" 2>&1
# The machine's TPM: its state lives in $W/tpm across boots. swtpm ends with each VM.
TPM_ARGS=(-chardev socket,id=chrtpm,path="$W/tpm.sock" -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0)
tpm_on() {
  rm -f "$W/tpm.sock"
  swtpm socket --tpm2 --tpmstate dir="$W/tpm" --ctrl type=unixio,path="$W/tpm.sock" --daemon
  for _ in $(seq 20); do [ -S "$W/tpm.sock" ] && return 0; sleep 0.5; done; return 1
}

vm() { # NAME [extra qemu args]
  local name=$1; shift
  rm -f "$W/qmp"
  qemu-system-x86_64 -enable-kvm -m 6144 -cpu host -smp 4 -machine q35,smm=on \
    -global driver=cfi.pflash01,property=secure,value=on \
    -drive if=pflash,format=qcow2,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=qcow2,file="$W/vars.qcow2" \
    -drive file="$W/win.raw",format=raw,if=none,id=hd0 -device virtio-blk-pci,drive=hd0,bootindex=1 \
    "$@" \
    -device virtio-vga -display none -device vhost-vsock-pci,guest-cid=42 \
    -smbios type=11,value=io.systemd.credential.binary:tmpfiles.extra="$cred" \
    -netdev user,id=n0 -device virtio-net-pci,netdev=n0 \
    -qmp unix:"$W/qmp",server,nowait -serial file:"$OUT/$name-serial.log" \
    -daemonize -pidfile "$W/pid"
}
S() { ssh -q -i "$W/key" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 \
        -o ProxyCommand="socat - VSOCK-CONNECT:42:22" root@vm "$@"; }
shot() { python3 "$H/qmp.py" "$W/qmp" shot "$OUT/$1.png" || true; }
wait_ssh() { # TRIES
  for _ in $(seq "$1"); do S true 2>/dev/null && return 0; sleep 10; done; return 1
}
stop_vm() {
  [ -f "$W/pid" ] || return 0
  S 'systemctl poweroff' 2>/dev/null || true
  for _ in $(seq 30); do kill -0 "$(cat "$W/pid")" 2>/dev/null || return 0; sleep 2; done
  python3 "$H/qmp.py" "$W/qmp" quit || true; sleep 2
}
fail() { log "FAIL: $*"; shot fail; stop_vm; chmod -R a+rX "$OUT"; exit 1; }

log "boot the live ISO (Secure Boot on)"
tpm_on
vm live -drive file="$ISO",media=cdrom,readonly=on,if=none,id=cd0 -device ide-cd,drive=cd0,bootindex=0 "${TPM_ARGS[@]}"
sleep 45; shot live-0-boot
wait_ssh 60 || fail "the live system did not answer over SSH within 10 minutes"
log "live system up"
sleep 20; shot live-1-installer
S 'mokutil --sb-state; cat /proc/cmdline; findmnt /; ls -l /dev/tpm*; cat /sys/class/tpm/tpm0/tpm_version_major; systemctl --failed --no-legend' > "$OUT/live-system.txt" 2>&1 || true

log "probe"
S '/usr/libexec/arka-install probe' > "$OUT/probe.json" || fail "probe"
log "plan next to Windows, encrypted, unlocked by the TPM with a PIN"
S 'mkdir -p /run/arka-install && /usr/libexec/arka-install plan --disk /dev/vda --mode free --tpm --image oci:/usr/lib/arka-install/image --keymap us --out /run/arka-install/plan.json' \
  > "$OUT/plan.json" || fail "plan: $(tail -c 600 "$OUT/plan.json")"
log "apply"
rc=0
printf '%s\n' "$PIN" | S '/usr/libexec/arka-install apply --plan /run/arka-install/plan.json' > "$OUT/apply.out" 2> "$OUT/apply.err" || rc=$?
S 'cat /run/arka-install/install.log' > "$OUT/install.log" 2>&1 || true
S 'efibootmgr' > "$OUT/efibootmgr-live.txt" 2>&1 || true
shot live-2-after-apply
[ "$rc" = 0 ] || fail "apply exited $rc: $(tail -c 800 "$OUT/apply.out")"
RK=$(python3 -c 'import json,sys
for l in open(sys.argv[1]):
    try: e = json.loads(l)
    except ValueError: continue
    if e.get("event") == "done": print(e["result"].get("recovery_key") or "")' "$OUT/apply.out")
[ -n "$RK" ] || fail "apply showed no recovery key"
if grep -qF "$PIN" "$OUT/install.log" "$OUT/apply.out" "$OUT/apply.err"; then fail "the PIN is in a log"; fi
if grep -qF "$RK" "$OUT/install.log" "$OUT/apply.err"; then fail "the recovery key is in a log"; fi
# Only the screen gets the key: keep it out of the uploaded results.
sed -i "s/$RK/RECOVERY-KEY/" "$OUT/apply.out"
log "apply finished; PIN in no log, recovery key only in the result for the screen"
stop_vm

log "check Windows"
bash "$H/check-windows.sh" "$W/win.raw" "$W/gpt-before" | tee "$OUT/windows-check.txt" || fail "Windows changed"
sgdisk -p "$W/win.raw" > "$OUT/gpt-after.txt"

unlock() { # NAME SECRET WHAT
  for attempt in 1 2; do
    sleep 60; shot "$1-$attempt-prompt"
    python3 "$H/qmp.py" "$W/qmp" type "$2"
    wait_ssh 30 && return 0
  done
  fail "the installed system did not answer after the $3"
}

log "boot the installed disk with the TPM"
tpm_on
vm installed "${TPM_ARGS[@]}"
unlock installed "$PIN" PIN
log "installed system up (TPM + PIN)"
sleep 30; shot installed-3-running
S 'bootc status; echo; cat /proc/cmdline; echo; findmnt /sysroot; echo; efibootmgr; echo; cat /boot/grub2/custom.cfg; echo; mokutil --sb-state; systemctl is-system-running; echo; systemd-cryptenroll "$(blkid -t TYPE=crypto_LUKS -o device)"' \
  > "$OUT/installed-system.txt" 2>&1 || true
awk '/^  booted:/{b=1} /^  rollback:/{b=0} b && /imageDigest: sha256:/{f=1} END{exit !f}' "$OUT/installed-system.txt" || fail "bootc status did not report a booted image"
grep -q "Windows" "$OUT/installed-system.txt" || fail "no Windows entry in custom.cfg"
# The disk's keys: the TPM one and the recovery key, and no other (the one-time key is gone).
keys=$(awk '/^SLOT/{k=1; next} k && NF==2{print $2}' "$OUT/installed-system.txt" | sort | tr '\n' ' ')
[ "$keys" = "recovery tpm2 " ] || fail "the disk's keys are '$keys', not just the TPM and the recovery key"
stop_vm

log "boot the installed disk without a TPM: the recovery key"
vm recovery
unlock recovery "$RK" "recovery key"
log "installed system up (recovery key)"
stop_vm
chmod -R a+rX "$OUT"
log "PASS"
