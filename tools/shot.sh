#!/usr/bin/env bash
# Build the rebuild (nestpro) and take a screenshot of it on a virtual display.
#   tools/shot.sh out.png [extra nestpro args, e.g. --part-tab=machining]
# This machine has no desktop: Xvfb (unpacked in ~/.local/opt/xvfb, its
# xkbcomp path patched to /tmp/xkb) provides the display. Each run starts its
# own on a free display number and stops it afterwards, so agents taking
# screenshots at the same time never share or stop each other's display.
# SHOT_DISPLAY=:N uses a display that is already running instead.
set -euo pipefail
out="${1:?usage: shot.sh out.png [args...]}"; shift
out="$(realpath -m "$out")"
export PATH="$HOME/.cargo/bin:$PATH"
R="$HOME/.local/opt/xvfb/root"
APP="$(cd "$(dirname "$0")/../app" && pwd)"

display="${SHOT_DISPLAY:-}"
if [ -z "$display" ]; then
  mkdir -p /tmp/xkb && { [ -e /tmp/xkb/xkbcomp ] || ln -sf "$R/usr/bin/xkbcomp" /tmp/xkb/xkbcomp 2>/dev/null || true; }
  num="$(mktemp)"
  # -displayfd: Xvfb picks a free display number and writes it when ready.
  LD_LIBRARY_PATH="$R/usr/lib/x86_64-linux-gnu" "$R/usr/bin/Xvfb.patched" -displayfd 3 \
    -screen 0 2200x1400x24 -nolisten tcp 3>"$num" >/dev/null 2>&1 &
  xvfb=$!
  trap 'kill "$xvfb" 2>/dev/null; rm -f "$num"' EXIT
  for _ in $(seq 40); do [ -s "$num" ] && break; sleep 0.25; done
  [ -s "$num" ] || { echo "shot.sh: Xvfb did not start" >&2; exit 1; }
  display=":$(tr -d '[:space:]' < "$num")"
fi

cargo build --quiet --manifest-path "$APP/Cargo.toml"
cd "$APP"
cfg="$(mktemp -d)"   # fresh settings each shot, so results are repeatable
# A Lantek install for its machine files: $LANTEK_DIR, else vendor/Expert if
# present, else none (built-in values). Screenshots for the public repo must
# show demo data only, never jobs from a real install (AGENTS.md).
lantek="${LANTEK_DIR:-}"
[ -z "$lantek" ] && [ -d "$APP/../vendor/Expert" ] && lantek="$(cd "$APP/../vendor/Expert" && pwd)"
DISPLAY="$display" WAYLAND_DISPLAY= LANTEK_DIR="$lantek" XDG_CONFIG_HOME="$cfg" \
  timeout 120 ./target/debug/nestpro --screenshot "$out" "$@" 2>&1 | grep -v -e MESA -e DRI3 || true
rm -rf "$cfg"
ls -la "$out"
