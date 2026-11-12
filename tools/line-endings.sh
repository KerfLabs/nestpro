#!/usr/bin/env bash
# Some tracked files use CRLF line endings and the rest LF, and scripted
# rewrites quietly turn one into the other (a whole-file diff). Fail when a
# file changed since the branch left main (committed or not) now ends its
# lines differently from main's copy, or a new file mixes the two, and say
# how to put them back. Nothing is rewritten here.
#
#   tools/line-endings.sh          compare with where this branch left origin/main (or main)
#   tools/line-endings.sh REV      compare with REV
set -euo pipefail
cd "$(dirname "$0")/.."

base=${1:-}
if [ -z "$base" ]; then
  for b in origin/main main; do
    if git rev-parse -q --verify "$b^{commit}" >/dev/null; then base=$(git merge-base HEAD "$b") && break; fi
  done
fi
base=${base:-HEAD}

# CRLF, LF, mixed, or none (no line breaks), of file $1.
style() {
  local all crlf
  all=$(LC_ALL=C tr -cd '\n' < "$1" | wc -c)
  crlf=$(LC_ALL=C grep -ac $'\r$' "$1" || true)
  if [ "$all" -eq 0 ]; then echo none
  elif [ "$crlf" -eq 0 ]; then echo LF
  elif [ "$crlf" -ge "$all" ]; then echo CRLF
  else echo mixed; fi
}
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

fail=0
# Text files only: numstat shows "-" for binary ones.
while IFS=$'\t' read -r added _ path; do
  [ "$added" = - ] && continue
  [ -f "$path" ] || continue
  now=$(style "$path")
  if git cat-file -e "$base:$path" 2>/dev/null; then
    git show "$base:$path" > "$tmp"
    was=$(style "$tmp")
  else
    was=new
  fi
  case "$was/$now" in
    */none|none/*|new/LF|new/CRLF) continue ;;
    "$now/$now") continue ;;
  esac
  fail=1
  if [ "$was" = new ]; then
    echo "line-endings: $path is new and mixes CRLF and LF. Make it one or the other: perl -pi -e 's/\\r\\n/\\n/' $path (LF) or perl -pi -e 's/\\r?\\n/\\r\\n/' $path (CRLF)"
    continue
  fi
  case "$was" in
    CRLF) fix="perl -pi -e 's/\\r?\\n/\\r\\n/' $path" ;;
    LF)   fix="perl -pi -e 's/\\r\\n/\\n/' $path" ;;
    *)    fix="start again from git show $(git rev-parse --short "$base"):$path and make the change without rewriting the whole file" ;;
  esac
  echo "line-endings: $path was $was, is $now now. Put its endings back: $fix"
done < <(git diff --numstat --diff-filter=AM "$base" --)

[ $fail = 0 ] && echo "line-endings: ok (against $(git rev-parse --short "$base"))"
exit $fail
