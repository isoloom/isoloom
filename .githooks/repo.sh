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

# gate: CI's rustfmt, typos and clippy jobs (.github/CONTRIBUTING.md, "Gates"): seconds, then clippy's minute.
gate() {
  cargo fmt --all --check || refuse "cargo fmt --all --check fails: cargo fmt --all, commit that, push again"
  command -v typos >/dev/null 2>&1 || refuse "typos isn't installed, and CI's spelling gate runs it: cargo install typos-cli"
  typos || refuse "typos finds a misspelling: fix it, or list the word in .typos.toml"
  # CI's clippy, as it runs it (-D warnings over every target): a minute when the build is warm,
  # and the refusal CI would give an hour later (main's gate was red for a day, 2026-11-13).
  cargo clippy --workspace --all-targets --locked -- -D warnings || refuse "cargo clippy --workspace --all-targets -- -D warnings fails: fix it, commit that, push again"
}
