#!/usr/bin/env bash
# On-device smoke test for arka-agent. Prints PASS/FAIL per step.
# Needs: a running Ollama with qwen2.5 pulled, and arkad on the system bus.
#   ./scripts/live-check.sh
set -uo pipefail
cd "$(dirname "$0")/.."

pass=0; fail=0
ok(){   echo "PASS  $1"; pass=$((pass+1)); }
no(){   echo "FAIL  $1"; fail=$((fail+1)); }

OLLAMA="${OLLAMA_URL:-http://127.0.0.1:11434}"

# 1. Ollama up
if curl -fsS "$OLLAMA/api/tags" >/dev/null 2>&1; then ok "Ollama reachable at $OLLAMA"; else no "Ollama not reachable at $OLLAMA"; fi

# 2. the supported model is pulled (there is no fallback model)
tags="$(curl -fsS "$OLLAMA/api/tags" 2>/dev/null || echo '')"
if printf '%s' "$tags" | grep -q 'qwen2.5:7b-instruct'; then ok "model qwen2.5:7b-instruct present"
else no "qwen2.5:7b-instruct not pulled (ollama pull qwen2.5:7b-instruct)"; fi

# 3. arkad on the system bus
if command -v busctl >/dev/null 2>&1 && busctl --system list 2>/dev/null | grep -q 'org.arka.arkad'; then
    ok "arkad on the system bus (org.arka.arkad)"
elif command -v gdbus >/dev/null 2>&1 && gdbus introspect --system --dest org.arka.arkad --object-path /org/arka/arkad >/dev/null 2>&1; then
    ok "arkad on the system bus (org.arka.arkad)"
else
    no "arkad not found on the system bus"
fi

# 4. build
if cargo build -q 2>/dev/null; then ok "cargo build"; else no "cargo build"; fi
BIN=target/debug/arka-agent

# 5. a read request answers
if [ -x "$BIN" ] && "$BIN" ask "is DNS-over-TLS on?" >/tmp/arka-live-read.out 2>&1; then
    ok "ask \"is DNS-over-TLS on?\" -> $(tail -1 /tmp/arka-live-read.out | cut -c1-60)"
else
    no "read request failed"; tail -3 /tmp/arka-live-read.out 2>/dev/null
fi

# 6. a write request, DRY-RUN first, auto-declined at the prompt (no real change)
DRYCFG="$(mktemp --suffix=.toml)"
printf 'dry_run = true\n' > "$DRYCFG"
if [ -x "$BIN" ] && printf 'n\n' | "$BIN" --config "$DRYCFG" ask "turn off MAC randomization" >/tmp/arka-live-write.out 2>&1; then
    ok "write request (dry-run) reached the approval prompt and was declined cleanly"
else
    no "write request (dry-run) errored"; tail -3 /tmp/arka-live-write.out 2>/dev/null
fi
rm -f "$DRYCFG"

# 7. audit chain verifies
if [ -x "$BIN" ] && "$BIN" log verify >/tmp/arka-live-verify.out 2>&1 && grep -q 'chain intact\|OK' /tmp/arka-live-verify.out; then
    ok "log verify -> $(cat /tmp/arka-live-verify.out)"
else
    no "log verify failed"; cat /tmp/arka-live-verify.out 2>/dev/null
fi

echo "---"
echo "live-check: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
