#!/usr/bin/env bash
# Build the rebuild (nestpro) and take a screenshot of it on a virtual display.
#   tools/shot.sh out.png [extra nestpro args, e.g. --part-tab=machining]
# This machine has no desktop: Xvfb (unpacked in ~/.local/opt/xvfb, its
# xkbcomp path patched to /tmp/xkb) provides display :97.
set -euo pipefail
out="${1:?usage: shot.sh out.png [args...]}"; shift
out="$(realpath -m "$out")"
export PATH="$HOME/.cargo/bin:$PATH"
R="$HOME/.local/opt/xvfb/root"
APP="$(cd "$(dirname "$0")/../app" && pwd)"

if [ ! -e /tmp/.X11-unix/X97 ]; then
  mkdir -p /tmp/xkb && ln -sf "$R/usr/bin/xkbcomp" /tmp/xkb/xkbcomp
  LD_LIBRARY_PATH="$R/usr/lib/x86_64-linux-gnu" setsid "$R/usr/bin/Xvfb.patched" :97 \
    -screen 0 2200x1400x24 -nolisten tcp >/tmp/xvfb97.log 2>&1 &
  for _ in $(seq 20); do [ -e /tmp/.X11-unix/X97 ] && break; sleep 0.5; done
fi

cargo build --quiet --manifest-path "$APP/Cargo.toml"
cd "$APP"
cfg="$(mktemp -d)"   # fresh settings each shot, so results are repeatable
# A Lantek install for its machine files: $LANTEK_DIR, else vendor/Expert if
# present, else none (built-in values). Screenshots for the public repo must
# show demo data only, never jobs from a real install (AGENTS.md).
lantek="${LANTEK_DIR:-}"
[ -z "$lantek" ] && [ -d "$APP/../vendor/Expert" ] && lantek="$(cd "$APP/../vendor/Expert" && pwd)"
DISPLAY=:97 WAYLAND_DISPLAY= LANTEK_DIR="$lantek" XDG_CONFIG_HOME="$cfg" \
  timeout 120 ./target/debug/nestpro --screenshot "$out" "$@" 2>&1 | grep -v -e MESA -e DRI3 || true
rm -rf "$cfg"
ls -la "$out"
