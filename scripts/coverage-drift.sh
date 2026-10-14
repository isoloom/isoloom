#!/usr/bin/env bash
# shellcheck disable=SC2016 # backticks in single quotes are Markdown, not command substitution
# Upstream drift: runs every scripts/refresh-* against the newest upstream releases and, when
# they add features, opens or updates one GitHub issue listing them.
#
# Every refresh script rewrites the feature lists the coverage tables are checked against (one
# feature per line). Run with ISOLOOM_REFRESH=latest, a script fetches the newest release
# instead of its pin; a script that can't simply refreshes its pin. A line added to a tracked
# file, or a new file, is a feature upstream added since the list was last committed.
#
# Usage: scripts/coverage-drift.sh [--dry-run] [--keep] [--scripts-dir DIR]
#   --dry-run       print the issue instead of calling gh (no token needed)
#   --keep          leave the refreshed lists in the working tree (default: restore them)
#   --scripts-dir   where the refresh-* scripts are (default: scripts/)
# The issue is the one open issue labelled coverage-drift; gh needs GH_TOKEN with issues: write.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPTS="$ROOT/scripts"
DRY_RUN=0
KEEP=0
LABEL="coverage-drift"
TITLE="Coverage: upstream added features"

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --keep) KEEP=1 ;;
    --scripts-dir) SCRIPTS="$(cd "$2" && pwd)"; shift ;;
    -h|--help) sed -n '3,15p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

cd "$ROOT"
if [ -n "$(git status --porcelain)" ]; then
  echo "the working tree has changes: commit them first, so the drift is only upstream's" >&2
  exit 2
fi

WORK="$(mktemp -d)"
restore() {
  if [ "$KEEP" -eq 0 ]; then
    git checkout -q -- .
    git clean -q -fd -- . >/dev/null
  fi
  rm -rf "$WORK"
}
trap restore EXIT

# Runs one refresh script: by its shebang when it's executable, else by its extension.
run_script() {
  if [ -x "$1" ]; then
    "$1"
  else
    case "$1" in
      *.py) python3 "$1" ;;
      *) bash "$1" ;;
    esac
  fi
}

ran=0
failed=""
for script in "$SCRIPTS"/refresh-*; do
  [ -f "$script" ] || continue
  name="$(basename "$script")"
  case "$name" in
    *.sh|*.py) ;;
    *) [ -x "$script" ] || { echo "== $name: skipped (not executable, not .sh or .py)"; continue; } ;;
  esac
  ran=$((ran + 1))
  echo "== $name"
  if ISOLOOM_REFRESH=latest run_script "$script" >"$WORK/$name.log" 2>&1; then
    sed 's/^/   /' "$WORK/$name.log"
  else
    sed 's/^/   /' "$WORK/$name.log"
    failed="$failed $name"
  fi
done
echo "$ran refresh scripts run"

# The features added: lines added to tracked files, and every line of new files.
added="$WORK/added.md"
: >"$added"
git diff --name-only >"$WORK/changed"
while IFS= read -r file; do
  lines="$(git diff -U0 -- "$file" | grep '^+[^+]' | sed 's/^+//' | grep -v '^#' || true)"
  if [ -n "$lines" ]; then
    printf '### `%s`\n\n%s\n\n' "$file" "$(printf '%s\n' "$lines" | sed 's/^/- `/; s/$/`/')" >>"$added"
  fi
done <"$WORK/changed"
git ls-files --others --exclude-standard >"$WORK/new"
while IFS= read -r file; do
  printf '### `%s` (new)\n\n%s\n\n' "$file" "$(grep -v '^#' "$file" | sed 's/^/- `/; s/$/`/')" >>"$added"
done <"$WORK/new"

if [ ! -s "$added" ] && [ -z "$failed" ]; then
  echo "no drift: every feature list matches upstream"
  exit 0
fi

body="$WORK/body.md"
{
  echo "The weekly drift job ran every \`scripts/refresh-*\` against the newest upstream releases."
  echo
  if [ -s "$added" ]; then
    echo "## Added upstream"
    echo
    echo "Classify each in its coverage table (run the refresh script with \`ISOLOOM_REFRESH=latest\`, then \`cargo test -p isoloom-coverage\`), and move the script's pin."
    echo
    cat "$added"
  fi
  if [ -n "$failed" ]; then
    echo "## Refresh scripts that failed"
    echo
    for name in $failed; do
      printf -- '- `%s`:\n\n```\n%s\n```\n\n' "$name" "$(tail -n 20 "$WORK/$name.log")"
    done
  fi
  if [ -n "${GITHUB_RUN_ID:-}" ]; then
    echo "Run: ${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-}/actions/runs/$GITHUB_RUN_ID"
  fi
} >"$body"

if [ "$DRY_RUN" -eq 1 ]; then
  echo "-- dry run: the issue \"$TITLE\" (label $LABEL) would read:"
  cat "$body"
  exit 0
fi

gh label create "$LABEL" --color FBCA04 --description "Upstream features the coverage tables don't list yet" --force >/dev/null
existing="$(gh issue list --label "$LABEL" --state open --limit 1 --json number --jq '.[0].number // empty')"
if [ -n "$existing" ]; then
  gh issue edit "$existing" --title "$TITLE" --body-file "$body"
  echo "updated issue #$existing"
else
  gh issue create --title "$TITLE" --label "$LABEL" --body-file "$body"
fi
