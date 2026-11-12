#!/usr/bin/env bash
# Every rule ID in app/MECHANICS.md's tables, and every row ID in FEATURES.md,
# names one thing. A duplicate is reported with both places, e.g.
#   unique-ids: ST1 twice in app/MECHANICS.md: line 1553 (How Lantek stores
#   nestings) and line 2082 (Sheet stock and remnants)
# Fix it by giving one of them a free ID and updating every reference
# (code comments, app/mechanics.json "source", FEATURES.md, research/).
set -euo pipefail
cd "$(dirname "$0")/.."

# file, then the pattern a row's first cell must match to count as an ID.
check() {
  tr -d '\r' < "$1" | awk -v file="$1" -v pat="$2" '
    /^#+ / { section = $0; sub(/^#+ +/, "", section) }
    /^\|/ {
      split($0, cell, "|"); id = cell[2]
      gsub(/^[ *]+|[ *]+$/, "", id)
      if (id !~ pat) next
      if (id in first) {
        printf "unique-ids: %s twice in %s: line %s (%s) and line %d (%s)\n", id, file, first[id], where[id], NR, section
        bad = 1
      } else { first[id] = NR; where[id] = section }
    }
    END { exit bad }'
}

fail=0
check app/MECHANICS.md '^[A-Z][A-Z]*(-[A-Z]+)?[0-9]+[a-z]?$' || fail=1
check FEATURES.md '^[A-Z]+-[0-9]+$' || fail=1
[ $fail = 0 ] && echo "unique-ids: ok"
exit $fail
