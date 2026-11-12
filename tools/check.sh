#!/usr/bin/env bash
# Check before every commit: nothing private in the files to publish, the
# tests pass on Linux, and the app type-checks for Windows (the main target).
# Windows linking and the .exe icon need a Windows machine.
# Exits 1 when any step fails, so scripts and agents can trust the exit code.
set -uo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

# cargo builds and tests in CARGO_TARGET_DIR when it is set. One outside this
# checkout may hold another worktree's build, so the tests could run that
# tree's binary and pass falsely.
here=$(pwd -P)
for v in CARGO_TARGET_DIR CARGO_BUILD_TARGET_DIR; do
  d=${!v:-}
  [ -n "$d" ] || continue
  case "$(cd app && realpath -m -- "$d")/" in
    "$here"/*) ;;
    *) echo "== $v=$d is outside this checkout ($here): cargo would test another tree's build. Run with it unset (env -u $v tools/check.sh) or inside this checkout."
       exit 1 ;;
  esac
done

# One check at a time per checkout: a second one waits instead of sharing app/target.
mkdir -p app/target
if command -v flock >/dev/null; then
  exec 9>app/target/.check.lock
  flock -n 9 || { echo "== Waiting for another check in this checkout"; flock 9; }
fi

failed=()
echo "== Public check";  tools/public-check.sh | tail -40
[ "${PIPESTATUS[0]}" = 0 ] || failed+=("public check")
echo "== Rule IDs";      tools/unique-ids.sh || failed+=("rule IDs")
echo "== Line endings";  tools/line-endings.sh || failed+=("line endings")
cd app
echo "== Linux tests";   cargo test 2>&1 | grep -E "^test result|FAILED|panicked|^error"
[ "${PIPESTATUS[0]}" = 0 ] || failed+=("Linux tests")
echo "== Windows check"; cargo check --target x86_64-pc-windows-gnu 2>&1 | grep -E "^(warning|error)|Finished"
[ "${PIPESTATUS[0]}" = 0 ] || failed+=("Windows check")

if [ ${#failed[@]} -gt 0 ]; then
  echo "== FAILED: ${failed[*]}"
  exit 1
fi
echo "== All checks passed"
