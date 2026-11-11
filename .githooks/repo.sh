# shellcheck shell=sh
# What this repository checks (lib.sh has what every hook shares): the staged files' formatting
# before a commit, the quickest of CI's gates before a push. Sourced by pre-commit and pre-push;
# refuse (lib.sh) stops git with the reason.

# format_staged <files>: rustfmt over the staged Rust (rustfmt.toml; the toolchain
# rust-toolchain.toml pins).
format_staged() {
  rust=$(printf '%s\n' "$1" | grep -E '\.rs$')
  if [ -n "$rust" ]; then
    # shellcheck disable=SC2086 # one file a word: the repository's paths have no spaces
    rustfmt --check --edition 2024 $rust || refuse "rustfmt would change a staged file: cargo fmt --all, then stage it again"
  fi
}

# gate: CI's rustfmt and typos jobs (.github/CONTRIBUTING.md, "Gates"), a few seconds.
gate() {
  cargo fmt --all --check || refuse "cargo fmt --all --check fails: cargo fmt --all, commit that, push again"
  command -v typos >/dev/null 2>&1 || refuse "typos isn't installed, and CI's spelling gate runs it: cargo install typos-cli"
  typos || refuse "typos finds a misspelling: fix it, or list the word in .typos.toml"
}
