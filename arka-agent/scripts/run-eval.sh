#!/usr/bin/env bash
# Run the arka-agent eval and print the pass rate.
#   ./scripts/run-eval.sh          # mock only (no GPU, no daemon)
#   ./scripts/run-eval.sh --live   # mock + live (needs Ollama with qwen2.5 pulled)
set -uo pipefail
cd "$(dirname "$0")/.."

echo "== arka-agent eval =="

# Mock: always runs, deterministic. Captures the "MOCK EVAL: x/20" line.
mock_out="$(cargo test --lib eval::eval_mock -- --exact --nocapture 2>&1)"
mock_line="$(printf '%s\n' "$mock_out" | grep -E '^MOCK EVAL:' || true)"
if [ -z "$mock_line" ]; then
    echo "mock: FAILED to run"
    printf '%s\n' "$mock_out" | tail -20
    exit 1
fi
echo "mock: ${mock_line#MOCK EVAL: }"

# Live: only with --live. Needs a running Ollama.
if [ "${1:-}" = "--live" ]; then
    live_out="$(cargo test --lib eval::eval_live -- --ignored --exact --nocapture 2>&1)"
    live_line="$(printf '%s\n' "$live_out" | grep -E '^LIVE EVAL:' || true)"
    if [ -z "$live_line" ]; then
        echo "live: FAILED to run (is Ollama up with qwen2.5 pulled?)"
        printf '%s\n' "$live_out" | tail -20
        exit 1
    fi
    echo "live: ${live_line#LIVE EVAL: }"
else
    echo "live: skipped (pass --live to run against Ollama)"
fi
