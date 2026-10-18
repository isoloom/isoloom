# Publishing Isoloom

The Isoloom crates are released to [crates.io](https://crates.io) so that consumers (the Isothrum
runner, the desktop app, the Cyber CTF launcher and any external user) depend on published
versions rather than git revisions. That way the 1.0 history squash can't break anyone, and each
consumer upgrades on its own schedule (see PLAN.md §0).

Everything here is licensed Apache-2.0.

## Publishable crates

All thirteen library/CLI crates are published, in this dependency order (leaf crates first; each
crate's dependencies are published before it):

1. `isoloom-spec` — the spec model, parsing, normalization and validation.
2. `isoloom-manifest` — the run manifest the Isothrum runner reads.
3. `isoloom-catalogue` — the embedded service catalogue (depends on spec).
4. `isoloom-plan` — the target-neutral plan (spec, catalogue).
5. `isoloom-render` — the shared rendering layer (spec, plan, manifest).
6. `isoloom-compose` — the Docker Compose backend.
7. `isoloom-kubernetes` — the Kubernetes backend.
8. `isoloom-terraform` — the Terraform backends.
9. `isoloom-vagrant` — the Vagrant backends.
10. `isoloom-ansible` — the Ansible backend.
11. `isoloom-targets` — the backend registry, targets, tiers and matrix (depends on every backend).
12. `isoloom-import` — drafting a spec from existing IaC (spec, plan).
13. `isoloom` — the command-line tool (spec, targets, import).

Not published (`publish = false`, the workspace default; each crate opts in with `publish = true`):

- `xtask` — the repo's development tasks; of no use to consumers.
- `fuzz` — the fuzz targets; a nightly/libFuzzer workspace, excluded from the main workspace.

## The catalogue crate

`isoloom-catalogue`'s `build.rs` embeds the repository's top-level `catalogue/` folder. That folder
lives outside the crate, so it would be missing from a published package and the embedded table
would be empty. To fix this, the crate carries a directory symlink `catalogue ->
../../../catalogue`: cargo copies the symlinked files into the package, and `build.rs` falls back
to that vendored copy (`$CARGO_MANIFEST_DIR/catalogue`) when the repository copy is absent. In the
repository it still reads the real `catalogue/`, so nothing changes for development.

## Versioning

All crates share one version through `[workspace.package].version` (currently `0.10.1` in the
working tree). Bump that single field and every crate moves together. The intra-workspace path
dependencies in `[workspace.dependencies]` each carry `version = "<the workspace version>"`
(crates.io rejects a bare path dependency); update those to match when you bump.

The first public release is the **`v0.9.0`** release candidate. Setting `0.9.0` is a coordinated
bump: it changes the version every Isothrum crate requests of Isoloom, so Isoloom and Isothrum
(and their committed lockfiles / dev-link pins) must move together. That bump is left to the
maintainer/coordinator rather than done piecemeal here.

## Authentication: crates.io Trusted Publishing (OIDC)

No long-lived API token is stored. `publish-crates.yml` authenticates with crates.io
[Trusted Publishing](https://crates.io/docs/trusted-publishing): the job declares
`permissions: id-token: write`, the official `rust-lang/crates-io-auth-action` mints a short-lived
token over GitHub's OIDC at run time, and `cargo publish` uses it for that run only.

**One-time setup by the maintainer, per publishable crate:** on crates.io, open the crate's
*Settings → Trusted Publishing* and add a GitHub publisher pointing at:

- repository owner/name: `isoloom/isoloom`
- workflow filename: `publish-crates.yml`
- environment: `crates-io` (the job runs in that environment)

**First publish of a new crate name.** Trusted Publishing cannot *create* a crate that does not
exist yet, so the very first publish of each new name must be done manually, once:

```
cargo login            # paste a token from https://crates.io/settings/tokens
cargo publish -p <crate>
```

after which the crate exists and the Trusted Publisher (added above) handles every later version
from the workflow. **New vs existing:** none of the thirteen crates are on crates.io yet, so all of
them (`isoloom-spec`, `-manifest`, `-catalogue`, `-plan`, `-render`, `-compose`, `-kubernetes`,
`-terraform`, `-vagrant`, `-ansible`, `-targets`, `-import`, `isoloom`) are new names and need the
one-time manual first publish for the `v0.9.0` release candidate. Confirm each name is free (or
already owned) before the first release. From `v0.9.1` onward the tag workflow does everything.

## Release flow

1. Bump `[workspace.package].version` and the matching `version`s in `[workspace.dependencies]`.
2. Commit, then tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The tag runs `.github/workflows/publish-crates.yml`, which publishes the thirteen crates in
   order and skips any version already on crates.io (idempotent — safe to re-run after a partial
   failure). The same tag also runs `release.yml` for the CLI binaries.
4. Publish Isoloom **before** Isothrum: the Isothrum crates depend on `isoloom-spec`,
   `isoloom-targets` and `isoloom-manifest` from crates.io at this version.
5. To publish without a tag, run the **Publish crates** workflow from the Actions tab
   (`workflow_dispatch`).

### Dry run

Every crate can be dry-run locally. The leaf crates verify against crates.io directly; the rest
depend on sibling Isoloom crates that aren't published yet, so point those at the local checkout
with a `[patch.crates-io]` passed to cargo (e.g. `cargo --config patch.toml publish --dry-run -p
<crate>`), where `patch.toml` maps each `isoloom-*` to its path.

```
cargo publish --dry-run -p isoloom-spec
cargo publish --dry-run -p isoloom-manifest
# …the remaining crates, in the order above, with the patch
```

## The npm / JavaScript side

Isoloom itself ships no JavaScript package; it publishes Rust crates only. The browser engine
(`isothrum-wasm`, a wasm-pack bundle) is released from the Isothrum repository, and the launcher's
shared UI package `@cyber-courses/ui` already ships through GitHub Packages. Neither goes to
crates.io.
