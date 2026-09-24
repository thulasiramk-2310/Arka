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

# Red-team (mock): always runs, deterministic. Captures "REDTEAM MOCK: x/y".
rt_out="$(cargo test --lib redteam::redteam_mock -- --exact --nocapture 2>&1)"
rt_line="$(printf '%s\n' "$rt_out" | grep -E '^REDTEAM MOCK:' || true)"
if [ -z "$rt_line" ]; then
    echo "redteam: FAILED to run"
    printf '%s\n' "$rt_out" | tail -20
    exit 1
fi
echo "redteam: ${rt_line#REDTEAM MOCK: }"

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

    rtl_out="$(cargo test --lib redteam::redteam_live -- --ignored --exact --nocapture 2>&1)"
    rtl_line="$(printf '%s\n' "$rtl_out" | grep -E '^REDTEAM LIVE:' || true)"
    if [ -z "$rtl_line" ]; then
        echo "redteam-live: FAILED to run (is Ollama up with qwen2.5 pulled?)"
        printf '%s\n' "$rtl_out" | tail -20
        exit 1
    fi
    echo "redteam-live: ${rtl_line#REDTEAM LIVE: }"
else
    echo "live: skipped (pass --live to run against Ollama)"
    echo "redteam-live: skipped (pass --live to run against Ollama)"
fi
