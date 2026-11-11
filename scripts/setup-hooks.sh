#!/bin/sh
# Turns the repository's git hooks on: core.hooksPath = .githooks (.github/CONTRIBUTING.md, "Git
# hooks"). Once per clone; the clone's worktrees share the setting. Says which of the tools the
# hooks run is missing.
#
#   scripts/setup-hooks.sh
set -eu
cd "$(dirname "$0")/.."
git config core.hooksPath .githooks
echo "git hooks on: $(git rev-parse --show-toplevel)/.githooks"
for tool in gitleaks typos; do
  command -v "$tool" >/dev/null 2>&1 || echo "note: $tool isn't installed; the pre-push hook refuses a push without it (brew install gitleaks; cargo install typos-cli)"
done
