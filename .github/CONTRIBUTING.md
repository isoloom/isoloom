# Contributing

Everyone follows the [code of conduct](CODE_OF_CONDUCT.md), and a vulnerability is reported as [SECURITY.md](SECURITY.md) says. How decisions are made: [GOVERNANCE.md](GOVERNANCE.md). Questions: [SUPPORT.md](SUPPORT.md).

## Gates

Every change passes these, locally and in CI. CI runs every one of them on each pull request.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings   # as CI runs it: any warning fails
cargo xtask quality                            # the ratchet: no per-crate count may go up
cargo xtask test                               # the tests under cargo-nextest, then the doctests
cargo deny check                               # advisories, licenses, bans, sources
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
cargo machete                                  # no unused dependencies
typos                                          # spelling; legitimate words go in .typos.toml
lychee --offline README.md CHANGELOG.md ARCHITECTURE.md '.github/*.md' 'docs/**/*.md'   # links

# The generated docs must stay current (their stale-file tests enforce this):
cargo run -q -p isoloom -- docs --spec > docs/spec.md
cargo run -q -p isoloom -- docs --matrix > docs/matrix.md
git diff --exit-code docs
```

Clippy runs with `-D warnings`, as CI does: any lint that warns fails it. A lint silenced where
the fix reads worse is an `#[expect(lint, reason = "…")]`, and `xtask/baselines/quality-baseline.json` counts
those per crate (`exception/expect`) with the ratchet's other counts: `cargo xtask quality` fails
if any count goes up. Drive counts down, never up.

When a change is meant to alter output, review the snapshot diffs with `cargo insta review`,
commit the accepted snapshots, and explain each kind of difference in the pull request.

Isoloom generates files; it runs nothing. To run an environment for real, generate it and hand
its run manifest to a runner ([the run manifest](../docs/reference/run-manifest.md)), or run each tool's files
by hand:

```sh
cargo build -q -p isoloom && export PATH=$PWD/target/debug:$PATH
cd examples/hello-stack && isoloom generate
# a runner reads .isoloom/docker/run.json (up, check, down); by hand:
(cd .isoloom/docker && docker compose up -d --build --wait && docker compose down)
```

## Conventions

- **Layout:** crates sit in groups under `crates/` — `core/`, `backends/`, `tools/`, `targets/`
  and `cli/`. Dependencies go one way (spec → plan → render → backends → targets → cli); a test
  (`cargo test -p xtask`) fails on any upward or cross-backend edge.
- **Shape:** functions at most 60 lines, at most 5 parameters (`clippy.toml`), one concept
  per module.
- **File size:** every source file holds at most 400 lines of code. Blank lines, comment-only
  lines and the inline `#[cfg(test)] mod tests` block don't count, and a separate `tests.rs` is
  not counted at all. A file over the limit is split into cohesive submodules, with `pub use`
  re-exports keeping its public paths. `cargo xtask quality` enforces it (`file/over-400-loc`).
- **Errors:** typed, with `thiserror`, one enum per library crate; variants carry the field
  path and reason, with a `#[source]` cause. No `String` errors, no `anyhow` in libraries.
- **No `unwrap`, `expect` or `panic`** outside tests (the restriction lints are at deny). A spec
  reaching a generator is validated: return an error for what validation should have caught,
  don't assume it. A truly impossible state is an `#[expect(lint, reason = "…")]`, never
  `#[allow]`.
- **Docs on public items:** `missing_docs` is on; say why, not what the signature already says,
  with `# Errors` and `# Panics` sections where they apply.
- **No copied helpers:** one quoting helper per language, centralised and property-tested. Need
  one in another crate? Move it down the pipeline and export it.
- **Deterministic output:** ordered maps (`IndexMap`, `BTreeMap`) in everything written, no
  timestamps or random names in generated files. A test renders every example twice.
- **Tests:** a unit test per behavior, named for the behavior; `insta` snapshots for whole
  rendered files only; `proptest` for parsing and quoting.
- **Comment style:** plain sentences, colons to introduce detail, no em dashes.
- **Lints** live in the root `Cargo.toml` `[lints]` table and are inherited by every crate
  (`lints.workspace = true`). Don't add an exception there; silence one locally only where the
  fix reads worse, with an `#[expect(lint, reason = "…")]` the ratchet counts.

## Commits and pull requests

- Commit titles are short and say what changed, like the existing history ("Volumes shared
  between machines", "libvirt runs a machine as its own CPU"). No attribution lines.
- A pull request says what changed, links the issue (`Closes #N`), pastes how it was verified
  and lists what was deliberately left out.

## Documentation

The docs live under `docs/`, by purpose ([index](../docs/README.md)): `guide/` (tutorials and
how-tos), `reference/`, `explanation/` and `dev/` (maintainers). `docs/spec.md`,
`docs/matrix.md` and `docs/reference/cli.md` are generated: change the code, then regenerate
them. Every link between the pages must resolve and every spec in a page (a YAML block whose
first line is `# isoloom.yml`) must validate: `cargo test -p isoloom-spec --test docs` checks both.

## License

By contributing, you agree that your contributions are licensed under the [Apache License, Version 2.0](../LICENSE), the project's license.
