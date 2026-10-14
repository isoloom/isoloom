#!/bin/sh
# Per-crate line coverage from a `cargo llvm-cov --json` export, checked against the floors in
# .github/coverage-floors.txt (`<crate> <percent>` per line). Prints a table (Markdown, for the
# job summary) and fails when a crate is under its floor. A crate without a floor is reported,
# not checked.
#
#   cargo llvm-cov --workspace --json --summary-only --output-path cov.json
#   scripts/coverage-floors.sh cov.json
set -eu

json=${1:?usage: coverage-floors.sh <llvm-cov json>}
floors=${2:-.github/coverage-floors.txt}
[ -f "$floors" ] || { echo "no floors file: $floors" >&2; exit 2; }

# Lines covered and counted per crate: files under crates/<crate>/src only (tests aren't measured).
per_crate=$(jq -r '
  .data[0].files[]
  | select(.filename | test("/crates/[^/]+/src/"))
  | [(.filename | capture("/crates/(?<c>[^/]+)/src/").c), .summary.lines.covered, .summary.lines.count]
  | @tsv' "$json" |
  awk -F '\t' '{ cov[$1] += $2; cnt[$1] += $3 } END { for (c in cov) printf "%s %.1f\n", c, (cnt[c] ? 100 * cov[c] / cnt[c] : 100) }' |
  sort)

status=0
echo "| crate | lines | floor |"
echo "|---|---:|---:|"
while read -r crate pct; do
  floor=$(awk -v c="$crate" '$1 == c { print $2 }' "$floors")
  if [ -z "$floor" ]; then
    echo "| $crate | $pct% | none |"
  elif awk -v p="$pct" -v f="$floor" 'BEGIN { exit !(p + 0 < f + 0) }'; then
    echo "| $crate | $pct% | $floor% **under** |"
    echo "::error title=coverage::$crate: $pct% of lines covered, under its floor of $floor%" >&2
    status=1
  else
    echo "| $crate | $pct% | $floor% |"
  fi
done <<EOF
$per_crate
EOF
exit $status
