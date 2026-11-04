#!/usr/bin/env bash
# Guard for the public repo: fail if a file that would be published carries
# reverse-engineering evidence, shop data or an unreviewed image.
#
#   tools/public-check.sh            check every tracked file (and new, not ignored ones)
#   tools/public-check.sh --staged   check what is staged (the pre-commit hook)
#
# What it blocks (see AGENTS.md "Keeping the public repo clean"):
#   - code addresses (0x14xxxxxxx / 0x18xxxxxxx), decompiler names (FUN_…,
#     local_…, param_…), analysis evidence and unknown ids (ev_…, unk_…)
#   - names of the original's binaries used as evidence (ExpertPr.exe, Mecaniza.dll, …)   public-check: allow
#   - personal paths (C:\Users\<name>, /home/<name>) and e-mail addresses
#   - paths that must never be committed: research/, vendor/, history/
#   - images not listed (with their sha256) in tools/public-images.txt
#   - anything in research/public-denylist.txt (private: shop names, servers,
#     job and part numbers, people). That list lives in the private research
#     repo, so CI runs without it; run this locally with research/ checked out.
#
# A line that must keep a match (e.g. this script) can end with the marker
# `public-check: allow`. Use it rarely; reviewers grep for it.
set -euo pipefail
cd "$(dirname "$0")/.."

staged=0
[ "${1:-}" = "--staged" ] && staged=1

if [ $staged = 1 ]; then
  mapfile -t files < <(git diff --cached --name-only --diff-filter=ACMR)
  show() { git show ":$1"; }
else
  mapfile -t files < <(git ls-files --cached --others --exclude-standard)
  show() { cat -- "$1"; }
fi

fail=0
report() { echo "public-check: $1"; fail=1; }

# Text patterns (extended regex, case-insensitive where it matters).
patterns=(
  'address|\b0x1[48][0-9a-fA-F]{7}\b'
  'decompiler name|\b(FUN|DAT|LAB|PTR|SUB|thunk_FUN)_[0-9a-fA-F]{6,}\b'
  'decompiler variable|\b(local|param|in_stack|extraout|unaff)_[0-9a-fA-F]{1,4}\b'
  'evidence id|\b(ev|unk)_[0-9a-f]{8,}'
  "original's binary|\b(ExpertPr|DrawNest|Mecaniza|Mecaniz1|GeometrV|DIALOGWD|UsExpert|DATABASE|Lantek\.[A-Za-z0-9.]+)\.(exe|dll)\b"
  'personal path|[A-Za-z]:\\\\Users\\\\[^\\%$<{ ]+|/home/[a-z][a-z0-9_-]*/'
  'e-mail|[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+\.[A-Za-z.]{2,}'
)
# E-mails that are fine to publish.
email_ok='noreply@anthropic\.com|users\.noreply\.github\.com|@example\.(com|org)|noreply@'

deny=()
if [ -f research/public-denylist.txt ]; then
  mapfile -t deny < <(grep -v -e '^\s*#' -e '^\s*$' research/public-denylist.txt)
elif [ -z "${CI:-}" ]; then
  echo "public-check: note: research/public-denylist.txt not found; shop-data names not checked"
fi

images_list=tools/public-images.txt
for f in "${files[@]}"; do
  [ -n "$f" ] || continue
  case "$f" in
    research/*|vendor/*|history/*) report "$f: this folder is never published"; continue ;;
  esac
  if [ $staged = 0 ] && [ ! -e "$f" ]; then continue; fi   # deleted, not yet staged
  case "${f,,}" in
    *.png|*.jpg|*.jpeg|*.gif|*.webp|*.bmp|*.ico)
      sum="$(show "$f" | sha256sum | cut -d' ' -f1)"
      grep -qx "$sum  $f" "$images_list" 2>/dev/null \
        || report "$f: image not reviewed (add \"$sum  $f\" to $images_list after checking it shows no shop data or the original's screens)"
      continue ;;
  esac
  [ "$f" = "$images_list" ] && continue
  content="$(show "$f" 2>/dev/null | grep -v 'public-check: allow' || true)"
  # Skip binary files.
  if printf '%s' "$content" | head -c 8000 | grep -qP '\x00'; then continue; fi
  for p in "${patterns[@]}"; do
    name="${p%%|*}"; re="${p#*|}"
    hits="$(printf '%s\n' "$content" | grep -nE -- "$re" || true)"
    [ "$name" = "e-mail" ] && hits="$(printf '%s\n' "$hits" | grep -vE "$email_ok" || true)"
    [ -n "$hits" ] || continue
    while IFS= read -r h; do report "$f:${h%%:*}: $name: $(printf '%s' "${h#*:}" | grep -oE -- "$re" | head -1)"; done <<< "$hits"
  done
  for d in "${deny[@]}"; do
    hits="$(printf '%s\n' "$content" | grep -niF -- "$d" || true)"
    [ -n "$hits" ] || continue
    while IFS= read -r h; do report "$f:${h%%:*}: private name from the denylist"; done <<< "$hits"
  done
done

if [ $fail = 1 ]; then
  echo "public-check: FAILED. Move evidence to the private research repo, or replace shop data with made-up data." >&2
  exit 1
fi
echo "public-check: ok (${#files[@]} files)"
