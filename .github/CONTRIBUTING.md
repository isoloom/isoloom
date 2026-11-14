# Contributing

Everyone follows the [code of conduct](CODE_OF_CONDUCT.md), and a vulnerability is reported as [SECURITY.md](SECURITY.md) says. How decisions are made: [GOVERNANCE.md](GOVERNANCE.md). Questions: [SUPPORT.md](SUPPORT.md).

## Gates

Every change passes these, locally and in CI. CI runs every one of them on each pull request.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings   # as CI runs it: any warning fails
cargo xtask quality                            # the ratchet: no per-crate count may go up
cargo xtask test                               # the tests under cargo-nextest, then the doctests; fails if they leave anything in the temporary folder
cargo deny check                               # advisories, licenses, bans, sources
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
cargo machete                                  # no unused dependencies
typos                                          # spelling; legitimate words go in .typos.toml
lychee --offline README.md CHANGELOG.md ARCHITECTURE.md '.github/*.md' 'docs/**/*.md'   # links

# The generated files (docs/spec.md, docs/matrix.md, docs/reference/cli.md, schemas/) are current;
# a stale one fails naming the command, `.githooks/regenerate`, which rewrites them all:
.githooks/regenerate --check
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

## Git hooks

`.githooks/` holds the repository's git hooks; `scripts/setup-hooks.sh` turns them on
(`git config core.hooksPath .githooks`, once per clone; the clone's worktrees share it). They are
POSIX sh and quick:

- `commit-msg` refuses a message with an assistant's attribution (`Co-Authored-By`,
  `Claude-Session`, "Generated with", 🤖): the history carries none.
- `pre-commit` refuses a staged file over 5 MB that Git LFS doesn't hold, and runs rustfmt over
  the staged Rust.
- `pre-commit` also regenerates and stages the generated files (`.githooks/regenerate`: the
  docs `isoloom docs` prints and the schemas) when the clone opts in, since it builds:
  `git config hooks.regenerate true`.
- `pre-push` runs [gitleaks](https://github.com/gitleaks/gitleaks) over the commits being pushed
  (`brew install gitleaks`; without it the push is refused, not left unscanned), the attribution
  check over them, then `cargo fmt --all --check` and `typos`, in a few seconds. The other gates
  stay in CI.

`.githooks/test.sh` tests them in a throwaway repository; CI's `hooks` job runs it.
`git commit --no-verify` skips a hook when you must; the push still checks the commits.

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
  rendered files only; `proptest` for parsing and quoting. A test's scratch folder is a
  `tempfile::TempDir`, removed when the test ends (by a panic too): `cargo xtask test` runs the
  tests in a temporary folder of their own and fails when they leave anything in it
  (`ISOLOOM_KEEP_TEMP=1` keeps that folder to look at).
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
how-tos), `reference/` and `explanation/`. They are user documentation, published on the website:
they explain what the shipped code does and how to use it. `docs/spec.md`, `docs/matrix.md` and
`docs/reference/cli.md` are generated: change the code, then regenerate them
(`.githooks/regenerate`). Every link between the pages must resolve and every spec in a page (a
YAML block whose first line is `# isoloom.yml`) must validate: `cargo test -p isoloom-spec --test
docs` checks both.

The project's own process stays out of `docs/`: no decision records, no notes on how a change was
decided or tested, no CI or test-host internals, no links pinned to a commit. Maintainer steps
belong in this file ([releasing](#releasing-maintainers)). The same test fails when `docs/` has an
`adr/` or `dev/` folder, or a page holds one of these markers (case ignored):

- a decision record: `decision record`, `ADR 0…`/`ADR-0…`, `Status: Accepted` (or `Proposed`,
  `Superseded`), `we decided`, `we chose`;
- maintainer material: a path `docs/dev/`, a GitHub link pinned to a commit
  (`…/blob/<hex>/…`, `…/tree/<hex>/…`);
- test-host and workflow vocabulary: `real-host`, `self-hosted runner`, `test rig`, `the rig`,
  `worktree`, `dev branch`, `test gate`, `release gate`, `coding agent`, `AI agent`, `subagent`.

A page that needs one of these words to explain a feature says it another way.

## The catalogue (contributors)

Writing a catalogue entry, pinning it and proving its parity.

An entry is a folder `catalogue/<entry>/`:

| File | Written by | Holds |
|---|---|---|
| `entry.yml` | by hand | the recipe, the settings schema, the ports, the releases and the OSes each runs on |
| `lock.yml` | `cargo xtask catalogue refresh` | the pinned digests and exact package versions |
| `contract.yml` | by hand | the parity contract: the facts that must read the same in both forms |
| `files/` | by hand | the entry's own templates, each carrying an `SPDX-License-Identifier: Apache-2.0` header |

The catalogue is embedded in the binary at build time (text files only, under 1 MB in all);
generating never reads the disk or the network for it. Shared data lives beside the entries:
`catalogue/daemons.yml` (the daemons no entry owns, with their foreground forms) and
`catalogue/collections.yml` (the pinned Ansible Galaxy collections, referenced from Galaxy and
never embedded). The schemas `catalogue/entry.schema.json` and `catalogue/contract.schema.json`
are generated from the types.

An entry is checked when the catalogue loads: its recipe must resolve on every release and OS it
lists, every setting must be used by the recipe, a template's `from` must be a file of the entry,
and the contract must have a default case. These checks run in the test suite, so a malformed
entry fails the build.

### Pinning

```sh
cargo xtask catalogue refresh [<entry>…] [--check] [--update] [--propose]
```

`refresh` pins, through `curl`, each repository signing key (by fingerprint then SHA-256), each
official image (the tag's multi-platform digest and platforms), each vendor package's exact
version per OS, and the Galaxy collections (version, URL and SHA-256), writing `lock.yml`. A pin
that moved upstream fails loudly unless `--update` accepts it; `--check` writes nothing and exits
non-zero on any drift; `--propose` lists newer upstream releases without adding them.

### Proving parity

```sh
cargo xtask catalogue check <entry> --runner PATH [--release V] [--os OS]
    [--form container|vm|both] [--case NAME]... [--provider P] [--keep]
```

`catalogue check` runs the entry's parity contract for real: each case becomes an environment, generated
for `docker` and for `vagrant`, run through a runner (up, check, down), and a case
passes only when every check passes in both forms and every contract fact reads the same in both.
`--case` runs only the cases named (repeat it; default: every case), `--runner` (required) is the
runner's command line (one taking `up`, `check --json` and `down`),
`--provider` the Vagrant provider of the VM form (default: `virtualbox`, `libvirt` on Linux), and
`--keep` leaves each environment running after its checks.

## Releasing (maintainers)

How a maintainer releases Isoloom: the crates to crates.io and the `isoloom` binaries to a GitHub
release, both from one tag `vX.Y.Z`. Only maintainers push release tags
([governance](GOVERNANCE.md)).

### What a release publishes

| What | Workflow | Where |
|---|---|---|
| The 13 published crates | [`publish-crates.yml`](workflows/publish-crates.yml) | crates.io, through Trusted Publishing (OIDC, no stored token) |
| The `isoloom` binaries for five targets, the SBOM, `SHA256SUMS`, its Sigstore signature, build provenance | [`release.yml`](workflows/release.yml) | the GitHub release of the tag |

Both start with the release gate ([`gate.yml`](workflows/gate.yml): rustfmt, clippy,
the tests, the API docs, the quality ratchet). A tag that fails it publishes nothing.

`release.yml` then:

1. checks that the tag is `v` + the workspace version, and takes the release notes from the
   CHANGELOG's `## [X.Y.Z]` section (it fails when there is none);
2. builds `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`,
   `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`, each archive holding the binary,
   `LICENSE`, `NOTICE`, `README.md` and `THIRD-PARTY-LICENSES.md` (cargo-about). The Windows one
   links the C runtime in (`-C target-feature=+crt-static`), so a Windows without the Visual C++
   Redistributable starts it, and the build fails when `isoloom.exe` still imports
   `VCRUNTIME*.dll` or `MSVCP*.dll` (`.github/scripts/no-vc-runtime.sh`; CI's `windows-binary` job
   checks the same on every push);
3. writes the CycloneDX SBOM (`isoloom.cdx.json`) and `SHA256SUMS`, signs `SHA256SUMS` keylessly
   with cosign and attests the provenance of every file;
4. creates the GitHub release, in the `release` environment (where approvals can be required);
5. installs the published release with `packaging/install.sh` (Linux x86_64 and arm64, macOS) and
   with the Action (Linux, macOS, Windows), runs it, and checks the signature and the provenance
   as [verify a release](../docs/guide/verify-a-release.md) says. A failure here means the published
   release is broken: fix it with a patch release;
6. waits (up to 40 minutes) for the website to serve `schemas/spec-v1.schema.json` at its `$id`
   (`https://www.isoloom.com/schema/v1.json`) and every `www.isoloom.com` page the CLI and the
   schema link to (`/en/docs/configuration`, `/en/docs/spec`), and fails when one doesn't answer
   or the served schema differs. A failure here means editors and `--help` point to stale or dead
   pages: publish the website (below).

The asset names (`isoloom-<target>.tar.gz`, `.zip` on Windows) are read by `install.sh`, the
Action, cargo-binstall and the verification guide: change them together.

### Before the tag

1. **Version.** Raise `[workspace.package].version` and every `version` of the
   `[workspace.dependencies]` path dependencies (crates.io refuses a path dependency without
   one). The [policies](../docs/reference/policies.md) say which part to raise; CI's `semver` job
   fails when a supported crate breaks its API without a major.
2. **CHANGELOG.** Rename `## [Unreleased]` to `## [X.Y.Z] — YYYY-MM-DD`, with breaking items
   marked **Breaking:** and, for a major, a migration guide under `docs/guide/`. Open a new empty
   `## [Unreleased]`.
3. **Generated pages.** `.githooks/regenerate`, then `.githooks/regenerate --check` passes.
4. **Docs that name the version.** The examples in `README.md`, `action.yml`,
   `packaging/install.sh` and the guides name `vX.Y.Z` where a version is shown: update them for a
   new major.
5. **Green CI on `main`** for the commit you tag, including the Compose and Kubernetes runs.
6. **Dry run** the crates (below) when dependencies or packaging changed.

### Tag

```sh
git tag -a vX.Y.Z -m "Isoloom X.Y.Z"    # -s instead of -a to sign it
git push origin vX.Y.Z
```

Then watch both workflows. For a major release, move the Action's major tag once the release is
verified, so `isoloom/isoloom@vX` users get it:

```sh
git tag -f vX vX.Y.Z && git push -f origin vX
```

Publish Isoloom **before** the crates built on it (a runner, an app): they depend on Isoloom's
crates from crates.io at this version.

### Publish the website

The website (its own repository) builds its docs from this repository's `docs/` and serves the
committed schemas at their `$id`. The spec's `$id`, every example's modeline and the CLI's help
link there, so a release isn't done until the website shows it:

1. Once the tag is pushed, sync the website to the tag's `docs/` and `schemas/`, and deploy it.
2. Check that the schema URL and the CLI's doc links answer:

   ```sh
   curl -fsSL https://www.isoloom.com/schema/v1.json | jq -S . | diff - <(jq -S . schemas/spec-v1.schema.json)
   curl -fsS -o /dev/null https://www.isoloom.com/en/docs/configuration
   curl -fsS -o /dev/null https://www.isoloom.com/en/docs/spec
   ```

`release.yml`'s `website` job runs the same check on every `www.isoloom.com` link in the CLI's
and the schema's source, waiting for the deploy.

### The crates

All crates share one version. They are published in dependency order, and the workflow skips any
crate whose version is already on crates.io, so re-running it after a partial failure is safe:

1. `isoloom-manifest`, 2. `isoloom-spec`, 3. `isoloom-catalogue`, 4. `isoloom-plan`,
5. `isoloom-render`, 6. `isoloom-compose`, 7. `isoloom-kubernetes`, 8. `isoloom-terraform`,
9. `isoloom-vagrant`, 10. `isoloom-ansible`, 11. `isoloom-targets`, 12. `isoloom-import`,
13. `isoloom` (the command line).

Not published (`publish = false`, the workspace default): `xtask`, `xtask-quality` and the fuzz
workspace.

#### Trusted Publishing

No long-lived token is stored. `publish-crates.yml` runs in the `crates-io` environment with
`id-token: write`; `rust-lang/crates-io-auth-action` exchanges the OIDC token for a short-lived
crates.io token. Each crate's *Settings → Trusted Publishing* on crates.io names repository
`isoloom/isoloom`, workflow `publish-crates.yml` and environment `crates-io`.

A **new crate name** can't be created by Trusted Publishing: publish its first version by hand
(`cargo login`, `cargo publish -p <crate>`), then add the Trusted Publisher.

#### The catalogue in a package

`isoloom-catalogue`'s `build.rs` embeds the repository's `catalogue/` folder, which lives outside
the crate. The crate carries a symlink `catalogue -> ../../../catalogue`: cargo packages the files
it points at, and `build.rs` reads that copy when the repository's isn't there.

#### Dry run

```sh
cargo publish --dry-run -p isoloom-manifest
```

The other crates (`isoloom-spec` included: it builds on `isoloom-manifest`) depend on sibling crates that are not on crates.io at the new version until they
are published: point those at
the checkout with a `[patch.crates-io]` table passed to cargo
(`cargo --config patch.toml publish --dry-run -p <crate>`).

To publish the crates without a tag, run **Publish crates** from the Actions tab.

### After the release

- Check the release page: five archives, `isoloom.cdx.json`, `SHA256SUMS`,
  `SHA256SUMS.sigstore.json`, and the notes.
- Check that `release.yml`'s `website` job passed ([publish the website](#publish-the-website)).
- Bump the consumers (Isothrum's `isoloom-*` dependencies) when the run manifest or the supported
  API changed.
- A security fix: publish the GitHub security advisory ([SECURITY.md](SECURITY.md)).

## License

By contributing, you agree that your contributions are licensed under the [Apache License, Version 2.0](../LICENSE), the project's license.
