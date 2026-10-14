# Contributing

Read [ARCHITECTURE.md](ARCHITECTURE.md) first: it explains how a spec becomes files and where
a change belongs. The decisions behind the layout are in [docs/adr/](docs/adr/).

## Gates

Every pull request passes these, locally and in CI:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
cargo llvm-cov --workspace --summary-only     # per-crate floors: .github/coverage-floors.txt
cargo machete                                 # no unused dependencies
cargo deny check                              # advisories, licenses, bans, sources
typos                                         # spelling; legitimate words go in _typos.toml

# Every example with committed output is up to date.
for e in examples/*/; do [ -d "$e.isoloom" ] && cargo run -q -p isoloom -- check "$e"; done

# Regenerate every example and the coverage page: nothing may change unless the change is the point.
cargo run -q -- coverage --markdown > docs/COVERAGE.md
cargo run -q -- docs --spec > docs/spec.md
git diff --exit-code examples docs
```

When a change is meant to alter output, review the snapshot diffs with `cargo insta review`,
commit the accepted snapshots and regenerated examples, and explain each kind of difference in
the pull request. Coverage floors only go up: raise a crate's floor when you raise its coverage.

A change touching Compose, the plan or the runtime is also run for real:

```sh
cargo build -q -p isoloom && export PATH=$PWD/target/debug:$PATH
cd examples/hello-stack && isoloom run docker . && git diff --exit-code -- .isoloom \
  && isoloom test docker . && isoloom down docker .
```

## Conventions

- **Shape:** functions at most 60 lines, at most 5 parameters (`clippy.toml`), one concept
  per module.
- **File size:** every source file under `crates/*/src` holds at most 400 lines of code.
  Blank lines, comment-only lines and the inline `#[cfg(test)] mod tests` block don't count,
  and a separate `tests.rs` is not counted at all. A file over the limit is split into
  cohesive submodules (a long data table by section), with `pub use` re-exports keeping its
  public paths, or its tests move to a `tests.rs`. `crates/isoloom-spec/tests/code_size.rs`
  enforces it.
- **Errors:** typed, with `thiserror`, one enum per library crate; variants carry the field
  path and reason ([ADR 0004](docs/adr/0004-error-model.md)). No new `String` errors.
- **No `unwrap`, `expect` or `panic`** outside tests. A spec reaching a generator is validated:
  return an error for what validation should have caught, don't assume it.
- **Docs on public items:** say why, not what the signature already says.
- **No copied helpers:** one quoting helper per language (shell in `isoloom_spec::shell`, HCL
  and the others in `isoloom-render`), one address helper. Need it in another crate? Move it
  down the pipeline and export it.
- **Deterministic output:** ordered maps (`IndexMap`, `BTreeMap`) in everything written, no
  timestamps or random names in generated files. A test renders every example twice.
- **Tests:** a unit test per behavior; `insta` snapshots for rendered output; `proptest` for
  parsing and quoting.
- **Comment style:** plain sentences, colons to introduce detail, no em dashes.
- **Lints** are set in the root `Cargo.toml` and inherited by every crate
  (`[lints] workspace = true`), all at `deny`. Don't add an exception there; silence one locally
  only where the fix reads worse, with a one-line comment saying why.

## Commits and pull requests

- Commit titles are short and say what changed, like the existing history ("Volumes shared
  between machines", "libvirt runs a machine as its own CPU"). No attribution lines.
- A pull request says what changed, links the issue (`Closes #N`), pastes how it was verified
  and lists what was deliberately left out.
- Add a CHANGELOG line for anything a user or embedder notices; a breaking change to the
  supported library surface gets a "Breaking (library)" line.
