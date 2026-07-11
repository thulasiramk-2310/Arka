#!/bin/bash
# ArkaOS QA: boot the image N times headless, record reliability + timing.
# Usage: scripts/qa-boot-loop.sh [N] [disk.qcow2]
# Output: qa-results/boot-loop-<timestamp>.csv  (one row per boot)
#
# Pass criteria per boot: VM reaches graphical.target (sddm up) within
# TIMEOUT seconds, arkad is active, and no systemd units failed.

set -u
N=${1:-10}
DISK=${2:-output/qcow2/disk.qcow2}
TIMEOUT=120
CODE=OVMF_CODE_4M_f42.qcow2
VARS=OVMF_VARS_4M_f42.qcow2
OUT=qa-results
MON=4464   # dedicated monitor/serial ports so a dev VM can coexist
SER=4465

mkdir -p "$OUT"
CSV="$OUT/boot-loop-$(date +%Y%m%d-%H%M%S).csv"
echo "boot,result,secs_to_graphical,failed_units,arkad,notes" > "$CSV"

# Serial expect helper: send a command after logging in, capture output.
serial_cmd() {
    # $1 = command;  logs in as ram/arkaos, runs it, prints output
    python3 - "$1" <<'PYEOF'
import socket, sys, time

cmd = sys.argv[1]
s = socket.create_connection(("localhost", 4465), timeout=10)
s.settimeout(3)

def drain():
    buf = b""
    try:
        while True:
            d = s.recv(4096)
            if not d: break
            buf += d
    except socket.timeout:
        pass
    return buf.decode(errors="replace")

def send(line):
    s.sendall(line.encode() + b"\n")
    time.sleep(0.6)

send("")                       # wake the getty
out = drain()
if "login:" in out:
    send("ram"); drain()
    send("arkaos"); time.sleep(1.5); drain()
send(cmd + " ; echo QA_EOC")
deadline = time.time() + 20
buf = ""
while time.time() < deadline and "QA_EOC" not in buf:
    buf += drain()
print(buf)
PYEOF
}

for i in $(seq 1 "$N"); do
    cp "$VARS" /tmp/qa-vars.qcow2
    qemu-system-x86_64 -enable-kvm -m 4096 -cpu host -smp 2 -machine q35 \
        -drive if=pflash,format=qcow2,readonly=on,file="$CODE" \
        -drive if=pflash,format=qcow2,file=/tmp/qa-vars.qcow2 \
        -drive file="$DISK",format=qcow2,if=virtio,snapshot=on \
        -device virtio-vga -display none \
        -serial telnet::$SER,server,nowait -monitor telnet::$MON,server,nowait &
    QPID=$!
    t0=$(date +%s)

    # Poll over serial until sddm/graphical target is up or timeout.
    result=FAIL; secs=""; failed=""; arkad=""
    while true; do
        now=$(date +%s); el=$(( now - t0 ))
        if (( el > TIMEOUT )); then result=TIMEOUT; break; fi
        st=$(serial_cmd "systemctl is-active graphical.target" 2>/dev/null)
        if grep -q "^active" <<< "$(grep -A1 graphical <<<"$st" | tail -1)" \
           || grep -qw active <<< "$st"; then
            secs=$el; result=PASS; break
        fi
        sleep 5
    done

    if [ "$result" = PASS ]; then
        failed=$(serial_cmd "systemctl --failed --no-legend | wc -l" | grep -oE '^[0-9]+' | tail -1)
        arkad=$(serial_cmd "systemctl is-active arkad" | grep -oE 'active|inactive|failed' | head -1)
        [ "${failed:-0}" != "0" ] && result=DEGRADED
        [ "$arkad" != "active" ] && result=DEGRADED
    fi

    kill $QPID 2>/dev/null; wait $QPID 2>/dev/null
    echo "$i,$result,${secs:-},${failed:-},${arkad:-}," | tee -a "$CSV"
done

echo
echo "Results: $CSV"
awk -F, 'NR>1{c[$2]++} END{for (k in c) printf "%s: %d\n", k, c[k]}' "$CSV"
