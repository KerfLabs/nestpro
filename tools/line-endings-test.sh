#!/usr/bin/env bash
# Tests for tools/line-endings.sh, in a throwaway repo: each case says what
# the check must answer (exit 0 ok, 1 a file to fix, 2 nothing to compare with).
set -uo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fails=0

# Away from this PC's own git settings (a global core.autocrlf, say).
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
git_() { git -c user.name=test -c user.email=test@example.com -c core.autocrlf=false "$@"; }

# A repo whose main has one CRLF file and one LF file; the check runs on a
# branch made from it.
repo() {
  rm -rf "$work/r" && mkdir -p "$work/r/tools" && cd "$work/r" || exit 1
  git_ init -q -b main . && cp "$here/tools/line-endings.sh" tools/
  printf 'a\r\nb\r\n' > crlf.txt && printf 'a\nb\n' > lf.txt
  git_ add -A && git_ commit -qm base && git_ switch -qc work
}

# expect CODE WORDS DESCRIPTION: run the check, want CODE and WORDS in what it says.
expect() {
  local out code
  out=$(tools/line-endings.sh "${refs[@]}" 2>&1); code=$?
  if [ "$code" != "$1" ] || ! grep -qF -- "$2" <<<"$out"; then
    echo "line-endings-test: FAIL $3: wanted exit $1 and \"$2\", got exit $code: $out"
    fails=$((fails + 1))
  fi
}
refs=()

repo; printf 'a\r\nb\r\nc\r\n' > crlf.txt; printf 'c\n' >> lf.txt
expect 0 "line-endings: ok" "changes that keep each file's endings"

repo; printf 'a\nb\n' > crlf.txt
expect 1 "crlf.txt was CRLF, is LF now" "a CRLF file rewritten with LF"

repo; git_ mv crlf.txt moved.txt; printf 'a\nb\n' > moved.txt
expect 1 "moved.txt was CRLF (as crlf.txt), is LF now" "a CRLF file renamed and rewritten with LF"

repo; git_ mv lf.txt moved.txt; git_ commit -qm move
expect 0 "line-endings: ok" "a file renamed with its endings kept, committed"

repo; printf 'a\r\nb\n' > new.txt; git_ add new.txt
expect 1 "new.txt is new and mixes CRLF and LF" "a new file with mixed endings"

repo; printf 'a\r\nb\n' > kept.txt; printf 'kept.txt -text\n' > .gitattributes; git_ add kept.txt .gitattributes
expect 0 "line-endings: ok" "a new file with mixed endings kept byte for byte (-text)"

repo; printf 'a\r\nb\r\n' > lf.txt
expect 1 "lf.txt was LF, is CRLF now" "an LF file rewritten with CRLF"
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true \
  expect 0 "line-endings: ok" "an LF file checked out with CRLF by core.autocrlf"

repo; git_ branch -qm main trunk
expect 2 "can't find origin/main main" "no main branch to compare with"

# A shallow clone (as CI checks out by default) has no history shared with main.
repo; printf 'c\r\n' >> crlf.txt; git_ commit -qam more; git_ switch -q main; printf 'z\n' >> lf.txt; git_ commit -qam main2
git_ clone -q --depth 1 --no-single-branch "file://$work/r" "$work/s" && cd "$work/s" && git_ switch -q work
expect 2 "no history shared with origin/main" "a shallow clone"
refs=(origin/main)
expect 2 "no history shared with origin/main" "a shallow clone, base named"
rm -rf "$work/s"

[ $fails = 0 ] && echo "line-endings-test: ok"
[ $fails = 0 ]
