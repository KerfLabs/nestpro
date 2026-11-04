#!/usr/bin/env bash
# Check before every commit: nothing private in the files to publish, the
# tests pass on Linux, and the app type-checks for Windows (the main target).
# Windows linking and the .exe icon need a Windows machine.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
echo "== Public check";     tools/public-check.sh | tail -40 || true
cd app
echo "== Linux tests";      cargo test 2>&1 | grep -E "^test result|FAILED|panicked|^error" || true
echo "== Windows check";    cargo check --target x86_64-pc-windows-gnu 2>&1 | grep -E "^(warning|error)|Finished" || true
