#!/usr/bin/env bash
# Some tracked files use CRLF line endings and the rest LF, and scripted
# rewrites quietly turn one into the other (a whole-file diff). Fail when a
# file changed since the branch left main (committed or not, renamed or
# not) now ends its lines differently from main's copy, or a new file mixes
# the two, and say how to put them back. Nothing is rewritten here. Fails,
# rather than passing, when it can't find what to compare with (a shallow
# clone, or no main branch fetched).
#
#   tools/line-endings.sh          compare with where this branch left origin/main (or main)
#   tools/line-endings.sh REF      compare with where it left REF (CI: a pull request's base)
set -euo pipefail
cd "$(dirname "$0")/.."

refs=("$@")
[ ${#refs[@]} -gt 0 ] || refs=(origin/main main)
base=
for b in "${refs[@]}"; do
  git rev-parse -q --verify "$b^{commit}" >/dev/null || continue
  base=$(git merge-base HEAD "$b") || {
    echo "line-endings: no history shared with $b to compare with (a shallow clone?). Fetch it in full: git fetch --unshallow origin, or fetch-depth: 0 in CI."
    exit 2
  }
  break
done
[ -n "$base" ] || { echo "line-endings: can't find ${refs[*]} to compare with. Fetch it first: git fetch origin main"; exit 2; }

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
# A throwaway index holding the base: adding the changed files to it gives
# what git would commit, with the same line-ending conversion as a real
# commit (core.autocrlf on a Windows checkout).
idx=$(mktemp)
trap 'rm -f "$tmp" "$idx"' EXIT
GIT_INDEX_FILE=$idx git read-tree "$base"

fail=0
# Text files only: numstat shows "-" for binary ones. A renamed file comes
# as an empty path and then its old and new names; it is compared with its
# old name's copy.
while IFS=$'\t' read -r -d '' added _ path; do
  from=$path
  if [ -z "$path" ]; then
    IFS= read -r -d '' from
    IFS= read -r -d '' path
  fi
  [ "$added" = - ] && continue
  [ -f "$path" ] || continue
  GIT_INDEX_FILE=$idx git add -- "$path" 2>/dev/null
  GIT_INDEX_FILE=$idx git cat-file blob ":$path" > "$tmp"
  now=$(style "$tmp")
  if git cat-file -e "$base:$from" 2>/dev/null; then
    git show "$base:$from" > "$tmp"
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
    *)    fix="start again from git show $(git rev-parse --short "$base"):$from and make the change without rewriting the whole file" ;;
  esac
  [ "$from" = "$path" ] || was="$was (as $from)"
  echo "line-endings: $path was $was, is $now now. Put its endings back: $fix"
done < <(git diff -z --numstat -M --diff-filter=AMR "$base" --)

[ $fail = 0 ] && echo "line-endings: ok (against $(git rev-parse --short "$base"))"
exit $fail
